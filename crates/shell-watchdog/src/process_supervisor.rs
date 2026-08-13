use std::ffi::OsString;
use std::io::{self, BufRead, BufReader, Write};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use shell_core::{
    ArmDeniedReason, MAX_WATCHDOG_CONTROL_FRAME_BYTES, RecoveryTransactionId,
    TaskbarStateFingerprint, WatchdogControlFrame, WatchdogFrameDirection,
};

#[cfg(windows)]
use crate::create_prepared_recovery_journal;
use crate::{
    ChildEvent, RestartDelay, SupervisionLifecycle, SupervisorAction, SupervisorConfig,
    SupervisorState, TaskbarArmingCoordinator, load_recovery_journal,
    mark_recovery_journal_applied, restore_recovery_journal,
};

const HEARTBEAT_LINE: &str = "MINHA_UI_HEARTBEAT v1";
const CONTROL_PREFIX: &[u8] = b"MINHA_UI_CONTROL";
const CHILD_OUTPUT_QUEUE_CAPACITY: usize = 16;
const SESSION_EXCLUSIVITY_RECHECK_INTERVAL: Duration = Duration::from_secs(2);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessSupervisorConfig {
    startup_timeout: Duration,
    heartbeat_timeout: Duration,
    healthy_window: Duration,
    poll_interval: Duration,
    suspend_jitter: Duration,
    restart: SupervisorConfig,
}

impl ProcessSupervisorConfig {
    #[must_use]
    pub const fn production() -> Self {
        Self {
            startup_timeout: Duration::from_secs(45),
            heartbeat_timeout: Duration::from_secs(8),
            healthy_window: Duration::from_secs(300),
            poll_interval: Duration::from_millis(250),
            suspend_jitter: Duration::from_secs(2),
            restart: SupervisorConfig::new(
                RestartDelay::from_millis(250),
                RestartDelay::from_millis(4_000),
                3,
            ),
        }
    }

    #[must_use]
    pub const fn with_timeouts(
        mut self,
        startup_timeout: Duration,
        heartbeat_timeout: Duration,
        healthy_window: Duration,
        poll_interval: Duration,
    ) -> Self {
        self.startup_timeout = startup_timeout;
        self.heartbeat_timeout = heartbeat_timeout;
        self.healthy_window = healthy_window;
        self.poll_interval = poll_interval;
        self
    }
}

impl Default for ProcessSupervisorConfig {
    fn default() -> Self {
        Self::production()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChildRun {
    Exited {
        status: ExitStatus,
        healthy_window_elapsed: bool,
    },
    Unresponsive {
        healthy_window_elapsed: bool,
    },
    Quiesced {
        healthy_window_elapsed: bool,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum ChildOutput {
    Heartbeat,
    Control(WatchdogControlFrame),
    ProtocolViolation,
}

/// Runs the shell process with bounded restart, heartbeat supervision and an
/// authenticated taskbar-mutation lease.
///
/// A pending journal is restored before the first spawn and after every child
/// exit, including clean exits, protocol errors and watchdog-forced kills. A
/// restoration failure stops supervision instead of launching another child.
pub fn supervise_process(
    executable: &Path,
    args: &[OsString],
    recovery_journal: &Path,
    config: ProcessSupervisorConfig,
) -> io::Result<()> {
    let lifecycle = SupervisionLifecycle::acquire(recovery_journal)?;
    restore_pending_journal(recovery_journal)?;
    let mut state = SupervisorState::default();
    loop {
        if lifecycle.shutdown_requested() {
            return Ok(());
        }
        let child = spawn_child(executable, args, false)?;
        let monitored =
            monitor_child_with_system_arming(child, config, recovery_journal, &lifecycle);
        // Deliberately unconditional: even a successful child may have crashed
        // between native restoration and its final DISARM frame.
        let restored = restore_pending_journal(recovery_journal);
        let run = combine_monitor_and_restore(monitored, restored)?;
        if lifecycle.shutdown_requested() {
            return Ok(());
        }
        if clean_exit(run) {
            return Ok(());
        }
        if healthy_window_elapsed(run) {
            let _ = state.observe(ChildEvent::HealthyWindowElapsed, config.restart);
        }
        let event = match run {
            ChildRun::Exited { .. } | ChildRun::Quiesced { .. } => ChildEvent::ChildCrashed,
            ChildRun::Unresponsive { .. } => ChildEvent::HeartbeatMissed,
        };
        match state.observe(event, config.restart) {
            SupervisorAction::Continue => {}
            SupervisorAction::Restart { delay } => {
                thread::sleep(Duration::from_millis(delay.millis()));
            }
            SupervisorAction::EnterSafeMode => {
                return run_safe_mode_once(executable, args, recovery_journal, config, &lifecycle);
            }
        }
    }
}

fn combine_monitor_and_restore(
    monitored: io::Result<ChildRun>,
    restored: io::Result<()>,
) -> io::Result<ChildRun> {
    match (monitored, restored) {
        (_, Err(error)) | (Err(error), Ok(())) => Err(error),
        (Ok(run), Ok(())) => Ok(run),
    }
}

fn run_safe_mode_once(
    executable: &Path,
    args: &[OsString],
    recovery_journal: &Path,
    config: ProcessSupervisorConfig,
    lifecycle: &SupervisionLifecycle,
) -> io::Result<()> {
    restore_pending_journal(recovery_journal)?;
    if lifecycle.shutdown_requested() {
        return Ok(());
    }
    let child = spawn_child(executable, args, true)?;
    let monitored = monitor_child_with_system_arming(child, config, recovery_journal, lifecycle);
    let restored = restore_pending_journal(recovery_journal);
    match combine_monitor_and_restore(monitored, restored)? {
        ChildRun::Exited { status, .. } if status.success() => Ok(()),
        ChildRun::Exited { status, .. } => Err(io::Error::other(format!(
            "safe-mode shell exited with {status}"
        ))),
        ChildRun::Unresponsive { .. } => {
            Err(io::Error::other("safe-mode shell stopped responding"))
        }
        ChildRun::Quiesced { .. } => Ok(()),
    }
}

fn spawn_child(executable: &Path, args: &[OsString], safe_mode: bool) -> io::Result<Child> {
    let mut command = Command::new(executable);
    command
        .args(args)
        .arg("--watchdog-child")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    if safe_mode && !args.iter().any(|argument| argument == "--safe-mode") {
        command.arg("--safe-mode");
    }
    command.spawn()
}

fn monitor_child_with_system_arming(
    child: Child,
    config: ProcessSupervisorConfig,
    recovery_journal: &Path,
    lifecycle: &SupervisionLifecycle,
) -> io::Result<ChildRun> {
    let mut backend = SystemTaskbarArmingBackend { recovery_journal };
    monitor_child_with_backend(child, config, &mut backend, Some(lifecycle))
}

fn monitor_child_with_backend<B: TaskbarArmingBackend>(
    mut child: Child,
    config: ProcessSupervisorConfig,
    backend: &mut B,
    lifecycle: Option<&SupervisionLifecycle>,
) -> io::Result<ChildRun> {
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("supervised shell stdout was not piped"))?;
    let mut stdin = child.stdin.take();
    let output = child_output_reader(stdout);
    let mut arming = TaskbarArmingCoordinator::idle();
    let started = Instant::now();
    let mut startup_anchor = started;
    let mut previous_poll = started;
    let mut first_heartbeat: Option<Instant> = None;
    let mut last_heartbeat: Option<Instant> = None;
    let mut last_session_exclusivity_check = started;
    loop {
        if lifecycle.is_some_and(SupervisionLifecycle::shutdown_requested) {
            let _ = child.kill();
            let _ = child.wait();
            return Ok(ChildRun::Quiesced {
                healthy_window_elapsed: first_heartbeat
                    .is_some_and(|heartbeat| heartbeat.elapsed() >= config.healthy_window),
            });
        }
        match child.try_wait() {
            Ok(Some(status)) => {
                return Ok(ChildRun::Exited {
                    status,
                    healthy_window_elapsed: first_heartbeat
                        .is_some_and(|heartbeat| heartbeat.elapsed() >= config.healthy_window),
                });
            }
            Ok(None) => {}
            Err(error) => return terminate_after_error(&mut child, error),
        }
        let now = Instant::now();
        if now.duration_since(previous_poll)
            >= config.poll_interval.saturating_add(config.suspend_jitter)
        {
            // Both processes are suspended together during sleep/resume. Grant
            // one fresh heartbeat window instead of treating scheduler time as
            // proof that the UI thread was hung.
            startup_anchor = now;
            if last_heartbeat.is_some() {
                last_heartbeat = Some(now);
            }
        }
        previous_poll = now;
        if arming.is_leased()
            && last_session_exclusivity_check.elapsed() >= SESSION_EXCLUSIVITY_RECHECK_INTERVAL
        {
            #[cfg(windows)]
            if let Err(error) = crate::ensure_current_user_session_exclusive() {
                return terminate_after_error(&mut child, error);
            }
            last_session_exclusivity_check = now;
        }
        for event in receive_child_output_batch(&output) {
            match event {
                ChildOutput::Heartbeat => {
                    let now = Instant::now();
                    first_heartbeat.get_or_insert(now);
                    last_heartbeat = Some(now);
                }
                ChildOutput::Control(frame) => {
                    let Some(input) = stdin.as_mut() else {
                        return terminate_after_error(
                            &mut child,
                            io::Error::new(
                                io::ErrorKind::BrokenPipe,
                                "supervised shell stdin was not piped",
                            ),
                        );
                    };
                    if let Err(error) = handle_control_frame(&mut arming, backend, input, frame) {
                        return terminate_after_error(&mut child, error);
                    }
                }
                ChildOutput::ProtocolViolation => {
                    return terminate_after_error(
                        &mut child,
                        io::Error::new(
                            io::ErrorKind::InvalidData,
                            "supervised shell violated the watchdog control protocol",
                        ),
                    );
                }
            }
        }
        let deadline_missed = last_heartbeat.map_or_else(
            || startup_anchor.elapsed() >= config.startup_timeout,
            |heartbeat| heartbeat.elapsed() >= config.heartbeat_timeout,
        );
        if deadline_missed {
            let healthy_window_elapsed = first_heartbeat
                .is_some_and(|heartbeat| heartbeat.elapsed() >= config.healthy_window);
            let _ = child.kill();
            let _ = child.wait();
            return Ok(ChildRun::Unresponsive {
                healthy_window_elapsed,
            });
        }
        thread::sleep(config.poll_interval);
    }
}

fn receive_child_output_batch(output: &Receiver<ChildOutput>) -> Vec<ChildOutput> {
    (0..CHILD_OUTPUT_QUEUE_CAPACITY)
        .map_while(|_| output.try_recv().ok())
        .collect()
}

fn terminate_after_error(child: &mut Child, error: io::Error) -> io::Result<ChildRun> {
    let _ = child.kill();
    let _ = child.wait();
    Err(error)
}

trait TaskbarArmingBackend {
    fn prepare(
        &mut self,
        fingerprint: TaskbarStateFingerprint,
    ) -> Result<RecoveryTransactionId, ArmDeniedReason>;

    fn mark_applied(
        &mut self,
        transaction_id: RecoveryTransactionId,
        expected_fingerprint: TaskbarStateFingerprint,
    ) -> io::Result<()>;

    fn restore(&mut self, transaction_id: RecoveryTransactionId) -> io::Result<()>;
}

struct SystemTaskbarArmingBackend<'a> {
    recovery_journal: &'a Path,
}

impl TaskbarArmingBackend for SystemTaskbarArmingBackend<'_> {
    fn prepare(
        &mut self,
        fingerprint: TaskbarStateFingerprint,
    ) -> Result<RecoveryTransactionId, ArmDeniedReason> {
        match load_recovery_journal(self.recovery_journal) {
            Ok(Some(_)) => return Err(ArmDeniedReason::PendingRecovery),
            Ok(None) => {}
            Err(_) => return Err(ArmDeniedReason::JournalUnavailable),
        }

        #[cfg(windows)]
        {
            crate::ensure_current_user_session_exclusive()
                .map_err(|_| ArmDeniedReason::UnsupportedState)?;
            let observation = crate::taskbar_restore_win32::observe_taskbar_arming_state()
                .map_err(|_| ArmDeniedReason::UnsupportedState)?;
            if observation.fingerprint != fingerprint {
                return Err(ArmDeniedReason::SnapshotMismatch);
            }
            let transaction_id = crate::taskbar_restore_win32::new_recovery_transaction_id()
                .map_err(|_| ArmDeniedReason::JournalUnavailable)?;
            create_prepared_recovery_journal(
                self.recovery_journal,
                transaction_id,
                observation.appbar_state,
                observation.snapshots,
            )
            .map_err(|_| ArmDeniedReason::JournalUnavailable)?;
            Ok(transaction_id)
        }

        #[cfg(not(windows))]
        {
            let _ = fingerprint;
            Err(ArmDeniedReason::UnsupportedState)
        }
    }

    fn mark_applied(
        &mut self,
        transaction_id: RecoveryTransactionId,
        expected_fingerprint: TaskbarStateFingerprint,
    ) -> io::Result<()> {
        #[cfg(windows)]
        {
            let current = crate::taskbar_restore_win32::observe_taskbar_arming_state()
                .map_err(io::Error::other)?;
            if current.fingerprint != expected_fingerprint {
                // Prepared means no LEASE was emitted and mutation was never
                // authorized. Cancel only this authenticated transaction; do
                // not apply its now-stale Explorer snapshot.
                authenticate_pending_transaction(self.recovery_journal, transaction_id)?;
                let journal = load_recovery_journal(self.recovery_journal)
                    .map_err(io::Error::other)?
                    .ok_or_else(|| {
                        io::Error::new(
                            io::ErrorKind::NotFound,
                            "Prepared recovery journal disappeared before cancellation",
                        )
                    })?;
                crate::taskbar_restore_win32::preflight_prepared_cancellation(&journal)
                    .map_err(io::Error::other)?;
                crate::cancel_prepared_recovery_journal(self.recovery_journal, transaction_id)
                    .map_err(io::Error::other)?;
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "taskbar state changed between Prepared and LEASE",
                ));
            }
        }

        #[cfg(not(windows))]
        let _ = expected_fingerprint;

        mark_recovery_journal_applied(self.recovery_journal, transaction_id)
            .map(|_| ())
            .map_err(io::Error::other)
    }

    fn restore(&mut self, transaction_id: RecoveryTransactionId) -> io::Result<()> {
        authenticate_pending_transaction(self.recovery_journal, transaction_id)?;
        restore_pending_journal(self.recovery_journal)
    }
}

fn authenticate_pending_transaction(
    recovery_journal: &Path,
    expected: RecoveryTransactionId,
) -> io::Result<()> {
    let journal = load_recovery_journal(recovery_journal)
        .map_err(io::Error::other)?
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "recovery journal is missing"))?;
    let found = journal
        .transaction_id
        .parse::<RecoveryTransactionId>()
        .map_err(io::Error::other)?;
    if found != expected {
        return Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            format!("recovery transaction mismatch: expected {expected}, found {found}"),
        ));
    }
    Ok(())
}

fn restore_pending_journal(recovery_journal: &Path) -> io::Result<()> {
    let pending = load_recovery_journal(recovery_journal)
        .map_err(io::Error::other)?
        .is_some();
    if !requires_user_session_exclusivity(pending, false) {
        return Ok(());
    }
    #[cfg(windows)]
    crate::ensure_current_user_session_exclusive()?;
    restore_recovery_journal(recovery_journal, false)
        .map(|_| ())
        .map_err(io::Error::other)
}

const fn requires_user_session_exclusivity(
    pending_recovery_journal: bool,
    taskbar_arming_requested: bool,
) -> bool {
    pending_recovery_journal || taskbar_arming_requested
}

fn handle_control_frame<B: TaskbarArmingBackend, W: Write>(
    arming: &mut TaskbarArmingCoordinator,
    backend: &mut B,
    output: &mut W,
    frame: WatchdogControlFrame,
) -> io::Result<()> {
    if frame.direction() != WatchdogFrameDirection::ChildToWatchdog {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "child sent a watchdog-to-child control frame",
        ));
    }
    match frame {
        WatchdogControlFrame::Arm {
            request_id,
            fingerprint,
        } => {
            let preparation = match arming.request_arm(request_id, fingerprint) {
                Ok(preparation) => preparation,
                Err(_) => {
                    return write_control_frame(
                        output,
                        WatchdogControlFrame::Denied {
                            request_id,
                            reason: ArmDeniedReason::Busy,
                        },
                    );
                }
            };
            let transaction_id = match backend.prepare(fingerprint) {
                Ok(transaction_id) => transaction_id,
                Err(reason) => {
                    return write_control_frame(
                        output,
                        WatchdogControlFrame::Denied { request_id, reason },
                    );
                }
            };
            let response = arming
                .commit_prepared(preparation, transaction_id)
                .map_err(io::Error::other)?;
            write_control_frame(output, response)
        }
        WatchdogControlFrame::Applied { transaction_id } => {
            let expected_fingerprint = arming
                .prepared_fingerprint(transaction_id)
                .map_err(io::Error::other)?;
            // The transition is synchronized before LEASE is emitted. The
            // child cannot enter its mutation adapter without that frame.
            backend.mark_applied(transaction_id, expected_fingerprint)?;
            let response = arming
                .commit_leased(transaction_id)
                .map_err(io::Error::other)?;
            write_control_frame(output, response)
        }
        WatchdogControlFrame::Cancel { request_id } => {
            let transaction_id = arming
                .request_cancel(request_id)
                .map_err(io::Error::other)?;
            backend.restore(transaction_id)?;
            let response = arming
                .commit_disarmed(transaction_id)
                .map_err(io::Error::other)?;
            write_control_frame(output, response)
        }
        WatchdogControlFrame::Disarm { transaction_id } => {
            arming
                .request_disarm(transaction_id)
                .map_err(io::Error::other)?;
            backend.restore(transaction_id)?;
            let response = arming
                .commit_disarmed(transaction_id)
                .map_err(io::Error::other)?;
            write_control_frame(output, response)
        }
        WatchdogControlFrame::Armed { .. }
        | WatchdogControlFrame::Denied { .. }
        | WatchdogControlFrame::Lease { .. }
        | WatchdogControlFrame::Disarmed { .. } => Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "child sent a watchdog-to-child control frame",
        )),
    }
}

fn write_control_frame(output: &mut impl Write, frame: WatchdogControlFrame) -> io::Result<()> {
    writeln!(output, "{}", frame.encode())?;
    output.flush()
}

fn child_output_reader(stdout: impl io::Read + Send + 'static) -> Receiver<ChildOutput> {
    let (sender, receiver) = mpsc::sync_channel(CHILD_OUTPUT_QUEUE_CAPACITY);
    let _reader = thread::spawn(move || {
        let mut reader = BufReader::new(stdout);
        let mut frame = Vec::with_capacity(HEARTBEAT_LINE.len());
        let mut oversized = false;
        while let Ok(buffer) = reader.fill_buf() {
            if buffer.is_empty() {
                break;
            }
            let consumed = buffer.len();
            for byte in buffer {
                if *byte == b'\n' {
                    if frame.last() == Some(&b'\r') {
                        let _ = frame.pop();
                    }
                    publish_child_output(&sender, &frame, oversized);
                    frame.clear();
                    oversized = false;
                } else if frame.len() < MAX_WATCHDOG_CONTROL_FRAME_BYTES {
                    frame.push(*byte);
                } else {
                    oversized = true;
                }
            }
            reader.consume(consumed);
        }
    });
    receiver
}

fn publish_child_output(sender: &mpsc::SyncSender<ChildOutput>, frame: &[u8], oversized: bool) {
    if !oversized && frame == HEARTBEAT_LINE.as_bytes() {
        let _ = sender.try_send(ChildOutput::Heartbeat);
        return;
    }
    if frame.starts_with(CONTROL_PREFIX) {
        let event = if oversized {
            ChildOutput::ProtocolViolation
        } else {
            WatchdogControlFrame::parse(frame)
                .map(ChildOutput::Control)
                .unwrap_or(ChildOutput::ProtocolViolation)
        };
        let _ = sender.send(event);
    }
}

fn clean_exit(run: ChildRun) -> bool {
    matches!(run, ChildRun::Exited { status, .. } if status.success())
}

const fn healthy_window_elapsed(run: ChildRun) -> bool {
    match run {
        ChildRun::Exited {
            healthy_window_elapsed,
            ..
        }
        | ChildRun::Unresponsive {
            healthy_window_elapsed,
        }
        | ChildRun::Quiesced {
            healthy_window_elapsed,
        } => healthy_window_elapsed,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        CONTROL_PREFIX, ChildOutput, ProcessSupervisorConfig, TaskbarArmingBackend,
        TaskbarArmingCoordinator, child_output_reader, handle_control_frame,
        receive_child_output_batch, requires_user_session_exclusivity,
    };
    #[cfg(any(unix, windows))]
    use super::{ChildRun, monitor_child_with_backend};
    use shell_core::{
        ArmDeniedReason, ArmRequestId, RecoveryTransactionId, TaskbarStateFingerprint,
        WatchdogControlFrame,
    };
    use std::cell::RefCell;
    use std::io::{self, Cursor, Write};
    #[cfg(any(unix, windows))]
    use std::process::{Command, Stdio};
    use std::rc::Rc;
    use std::sync::mpsc::RecvTimeoutError;
    use std::time::Duration;

    #[test]
    fn production_timeouts_leave_room_for_native_startup_and_suspend_jitter() {
        let config = ProcessSupervisorConfig::production();

        assert_eq!(config.startup_timeout, Duration::from_secs(45));
        assert_eq!(config.heartbeat_timeout, Duration::from_secs(8));
        assert_eq!(config.healthy_window, Duration::from_secs(300));
        assert!(config.poll_interval <= Duration::from_millis(250));
        assert_eq!(config.suspend_jitter, Duration::from_secs(2));
    }

    #[test]
    fn stable_supervision_without_journal_does_not_require_session_exclusivity() {
        assert!(!requires_user_session_exclusivity(false, false));
        assert!(requires_user_session_exclusivity(true, false));
        assert!(requires_user_session_exclusivity(false, true));
    }

    #[test]
    fn reader_accepts_heartbeat_and_versioned_control_frames() {
        let arm = WatchdogControlFrame::Arm {
            request_id: request(1),
            fingerprint: TaskbarStateFingerprint::from_bytes([4; 32]),
        };
        let input = format!(
            "diagnostic\nMINHA_UI_HEARTBEAT v0\nMINHA_UI_HEARTBEAT v1\n{}\n",
            arm.encode()
        );
        let receiver = child_output_reader(Cursor::new(input));

        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(1)),
            Ok(ChildOutput::Heartbeat)
        );
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(1)),
            Ok(ChildOutput::Control(arm))
        );
        assert!(matches!(
            receiver.recv_timeout(Duration::from_millis(20)),
            Err(RecvTimeoutError::Disconnected | RecvTimeoutError::Timeout)
        ));
    }

    #[test]
    fn oversized_control_frame_is_rejected_without_losing_the_next_heartbeat() {
        let mut input = CONTROL_PREFIX.to_vec();
        input.extend_from_slice(&vec![b'x'; 4_096]);
        input.extend_from_slice(b"\nMINHA_UI_HEARTBEAT v1\r\n");

        let receiver = child_output_reader(Cursor::new(input));

        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(1)),
            Ok(ChildOutput::ProtocolViolation)
        );
        assert_eq!(
            receiver.recv_timeout(Duration::from_secs(1)),
            Ok(ChildOutput::Heartbeat)
        );
    }

    #[test]
    fn bounded_control_queue_capacity_prevents_unbounded_poll_work() {
        const { assert!(super::CHILD_OUTPUT_QUEUE_CAPACITY <= 32) };
        let (sender, receiver) = std::sync::mpsc::sync_channel(64);
        for _ in 0..64 {
            sender.send(ChildOutput::Heartbeat).expect("queue input");
        }

        let batch = receive_child_output_batch(&receiver);

        assert_eq!(batch.len(), super::CHILD_OUTPUT_QUEUE_CAPACITY);
        assert_eq!(receiver.try_recv(), Ok(ChildOutput::Heartbeat));
    }

    #[test]
    fn persistence_and_restore_finish_before_authorization_responses() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut backend = RecordingBackend::new(Rc::clone(&events));
        let mut output = RecordingWriter::new(Rc::clone(&events));
        let mut coordinator = TaskbarArmingCoordinator::idle();
        let transaction_id = transaction(8);
        backend.transaction_id = transaction_id;

        handle_control_frame(
            &mut coordinator,
            &mut backend,
            &mut output,
            WatchdogControlFrame::Arm {
                request_id: request(7),
                fingerprint: TaskbarStateFingerprint::from_bytes([6; 32]),
            },
        )
        .expect("prepare handshake");
        handle_control_frame(
            &mut coordinator,
            &mut backend,
            &mut output,
            WatchdogControlFrame::Applied { transaction_id },
        )
        .expect("lease handshake");
        handle_control_frame(
            &mut coordinator,
            &mut backend,
            &mut output,
            WatchdogControlFrame::Disarm { transaction_id },
        )
        .expect("disarm handshake");

        assert_eq!(
            events.borrow().as_slice(),
            ["prepare", "write", "applied", "write", "restore", "write"]
        );
        let encoded = String::from_utf8(output.bytes).expect("ASCII frames");
        assert!(encoded.contains(" ARMED "));
        assert!(encoded.contains(" LEASE "));
        assert!(encoded.contains(" DISARMED "));
    }

    #[test]
    fn fingerprint_mismatch_never_publishes_armed() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut backend = RecordingBackend::new(Rc::clone(&events));
        backend.denial = Some(ArmDeniedReason::SnapshotMismatch);
        let mut output = RecordingWriter::new(Rc::clone(&events));
        let mut coordinator = TaskbarArmingCoordinator::idle();

        handle_control_frame(
            &mut coordinator,
            &mut backend,
            &mut output,
            WatchdogControlFrame::Arm {
                request_id: request(9),
                fingerprint: TaskbarStateFingerprint::from_bytes([1; 32]),
            },
        )
        .expect("denial is a valid response");

        assert_eq!(events.borrow().as_slice(), ["prepare", "write"]);
        let encoded = String::from_utf8(output.bytes).expect("ASCII frame");
        assert!(encoded.contains(" DENIED "));
        assert!(!encoded.contains(" ARMED "));
        assert_eq!(coordinator.transaction_id(), None);
    }

    #[test]
    fn state_change_after_prepared_never_publishes_lease() {
        let events = Rc::new(RefCell::new(Vec::new()));
        let mut backend = RecordingBackend::new(Rc::clone(&events));
        backend.fingerprint_at_applied = Some(TaskbarStateFingerprint::from_bytes([2; 32]));
        let mut output = RecordingWriter::new(Rc::clone(&events));
        let mut coordinator = TaskbarArmingCoordinator::idle();
        let transaction_id = transaction(12);
        let prepared_fingerprint = TaskbarStateFingerprint::from_bytes([1; 32]);
        backend.transaction_id = transaction_id;

        handle_control_frame(
            &mut coordinator,
            &mut backend,
            &mut output,
            WatchdogControlFrame::Arm {
                request_id: request(11),
                fingerprint: prepared_fingerprint,
            },
        )
        .expect("Prepared journal is acknowledged");
        let error = handle_control_frame(
            &mut coordinator,
            &mut backend,
            &mut output,
            WatchdogControlFrame::Applied { transaction_id },
        )
        .expect_err("changed taskbar state must deny a lease");

        assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        assert_eq!(events.borrow().as_slice(), ["prepare", "write", "applied"]);
        let encoded = String::from_utf8(output.bytes).expect("ASCII frames");
        assert!(encoded.contains(" ARMED "));
        assert!(!encoded.contains(" LEASE "));
        assert_eq!(coordinator.transaction_id(), Some(transaction_id));
    }

    #[cfg(any(unix, windows))]
    #[test]
    fn unresponsive_child_is_terminated_after_the_startup_deadline() {
        let child = spawn_unresponsive_child();
        let config = ProcessSupervisorConfig::production().with_timeouts(
            Duration::from_millis(25),
            Duration::from_millis(25),
            Duration::from_secs(1),
            Duration::from_millis(2),
        );
        let mut backend = RecordingBackend::new(Rc::new(RefCell::new(Vec::new())));

        let run = monitor_child_with_backend(child, config, &mut backend, None)
            .expect("monitor test child");

        assert_eq!(
            run,
            ChildRun::Unresponsive {
                healthy_window_elapsed: false
            }
        );
    }

    fn request(value: u64) -> ArmRequestId {
        ArmRequestId::try_new(value).expect("non-zero request id")
    }

    fn transaction(value: u8) -> RecoveryTransactionId {
        RecoveryTransactionId::try_from_bytes([value; 16]).expect("non-zero transaction id")
    }

    struct RecordingBackend {
        events: Rc<RefCell<Vec<&'static str>>>,
        transaction_id: RecoveryTransactionId,
        denial: Option<ArmDeniedReason>,
        fingerprint_at_applied: Option<TaskbarStateFingerprint>,
    }

    impl RecordingBackend {
        fn new(events: Rc<RefCell<Vec<&'static str>>>) -> Self {
            Self {
                events,
                transaction_id: transaction(1),
                denial: None,
                fingerprint_at_applied: None,
            }
        }
    }

    impl TaskbarArmingBackend for RecordingBackend {
        fn prepare(
            &mut self,
            _fingerprint: TaskbarStateFingerprint,
        ) -> Result<RecoveryTransactionId, ArmDeniedReason> {
            self.events.borrow_mut().push("prepare");
            self.denial.map_or(Ok(self.transaction_id), Err)
        }

        fn mark_applied(
            &mut self,
            _transaction_id: RecoveryTransactionId,
            expected_fingerprint: TaskbarStateFingerprint,
        ) -> io::Result<()> {
            self.events.borrow_mut().push("applied");
            if self
                .fingerprint_at_applied
                .is_some_and(|current| current != expected_fingerprint)
            {
                return Err(io::Error::new(
                    io::ErrorKind::PermissionDenied,
                    "taskbar state changed before Applied",
                ));
            }
            Ok(())
        }

        fn restore(&mut self, _transaction_id: RecoveryTransactionId) -> io::Result<()> {
            self.events.borrow_mut().push("restore");
            Ok(())
        }
    }

    struct RecordingWriter {
        events: Rc<RefCell<Vec<&'static str>>>,
        bytes: Vec<u8>,
        writing_frame: bool,
    }

    impl RecordingWriter {
        fn new(events: Rc<RefCell<Vec<&'static str>>>) -> Self {
            Self {
                events,
                bytes: Vec::new(),
                writing_frame: false,
            }
        }
    }

    impl Write for RecordingWriter {
        fn write(&mut self, buffer: &[u8]) -> io::Result<usize> {
            if !self.writing_frame {
                self.events.borrow_mut().push("write");
                self.writing_frame = true;
            }
            self.bytes.extend_from_slice(buffer);
            Ok(buffer.len())
        }

        fn flush(&mut self) -> io::Result<()> {
            self.writing_frame = false;
            Ok(())
        }
    }

    #[cfg(unix)]
    fn spawn_unresponsive_child() -> std::process::Child {
        Command::new("sh")
            .args(["-c", "while :; do :; done"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn test child")
    }

    #[cfg(windows)]
    fn spawn_unresponsive_child() -> std::process::Child {
        // `ping` provides a stock Windows process that remains alive without
        // writing a valid heartbeat. `monitor_child` must terminate it after
        // the bounded startup deadline instead of depending on a Unix shell.
        Command::new("ping.exe")
            .args(["-n", "30", "127.0.0.1"])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn Windows test child")
    }
}
