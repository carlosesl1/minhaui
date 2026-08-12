use std::ffi::OsString;
use std::io::{self, BufRead, BufReader};
use std::path::Path;
use std::process::{Child, Command, ExitStatus, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use crate::{ChildEvent, RestartDelay, SupervisorAction, SupervisorConfig, SupervisorState};

const HEARTBEAT_LINE: &str = "MINHA_UI_HEARTBEAT v1";
const MAX_PROTOCOL_FRAME_BYTES: usize = 128;

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
}

/// Runs the shell process with bounded restart and heartbeat supervision.
pub fn supervise_process(
    executable: &Path,
    args: &[OsString],
    config: ProcessSupervisorConfig,
) -> io::Result<()> {
    let mut state = SupervisorState::default();
    loop {
        let child = spawn_child(executable, args, false)?;
        let run = monitor_child(child, config)?;
        if clean_exit(run) {
            return Ok(());
        }
        if healthy_window_elapsed(run) {
            let _ = state.observe(ChildEvent::HealthyWindowElapsed, config.restart);
        }
        let event = match run {
            ChildRun::Exited { .. } => ChildEvent::ChildCrashed,
            ChildRun::Unresponsive { .. } => ChildEvent::HeartbeatMissed,
        };
        match state.observe(event, config.restart) {
            SupervisorAction::Continue => {}
            SupervisorAction::Restart { delay } => {
                thread::sleep(Duration::from_millis(delay.millis()));
            }
            SupervisorAction::EnterSafeMode => {
                return run_safe_mode_once(executable, args, config);
            }
        }
    }
}

fn run_safe_mode_once(
    executable: &Path,
    args: &[OsString],
    config: ProcessSupervisorConfig,
) -> io::Result<()> {
    let child = spawn_child(executable, args, true)?;
    match monitor_child(child, config)? {
        ChildRun::Exited { status, .. } if status.success() => Ok(()),
        ChildRun::Exited { status, .. } => Err(io::Error::other(format!(
            "safe-mode shell exited with {status}"
        ))),
        ChildRun::Unresponsive { .. } => {
            Err(io::Error::other("safe-mode shell stopped responding"))
        }
    }
}

fn spawn_child(executable: &Path, args: &[OsString], safe_mode: bool) -> io::Result<Child> {
    let mut command = Command::new(executable);
    command
        .args(args)
        .arg("--watchdog-child")
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit());
    if safe_mode && !args.iter().any(|argument| argument == "--safe-mode") {
        command.arg("--safe-mode");
    }
    command.spawn()
}

fn monitor_child(mut child: Child, config: ProcessSupervisorConfig) -> io::Result<ChildRun> {
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("supervised shell stdout was not piped"))?;
    let heartbeats = heartbeat_reader(stdout);
    let started = Instant::now();
    let mut startup_anchor = started;
    let mut previous_poll = started;
    let mut first_heartbeat: Option<Instant> = None;
    let mut last_heartbeat: Option<Instant> = None;
    loop {
        if let Some(status) = child.try_wait()? {
            return Ok(ChildRun::Exited {
                status,
                healthy_window_elapsed: first_heartbeat
                    .is_some_and(|heartbeat| heartbeat.elapsed() >= config.healthy_window),
            });
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
        while heartbeats.try_recv().is_ok() {
            let now = Instant::now();
            first_heartbeat.get_or_insert(now);
            last_heartbeat = Some(now);
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

fn heartbeat_reader(stdout: impl io::Read + Send + 'static) -> Receiver<()> {
    let (sender, receiver) = mpsc::sync_channel(1);
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
                    if !oversized && frame == HEARTBEAT_LINE.as_bytes() {
                        let _ = sender.try_send(());
                    }
                    frame.clear();
                    oversized = false;
                } else if frame.len() < MAX_PROTOCOL_FRAME_BYTES {
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
        } => healthy_window_elapsed,
    }
}

#[cfg(test)]
mod tests {
    #[cfg(unix)]
    use super::{ChildRun, monitor_child};
    use super::{ProcessSupervisorConfig, heartbeat_reader};
    use std::io::Cursor;
    #[cfg(unix)]
    use std::process::{Command, Stdio};
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
    fn reader_accepts_only_the_versioned_heartbeat_frame() {
        let receiver = heartbeat_reader(Cursor::new(
            b"diagnostic\nMINHA_UI_HEARTBEAT v0\nMINHA_UI_HEARTBEAT v1\n",
        ));

        assert_eq!(receiver.recv_timeout(Duration::from_secs(1)), Ok(()));
        assert!(matches!(
            receiver.recv_timeout(Duration::from_millis(20)),
            Err(RecvTimeoutError::Disconnected | RecvTimeoutError::Timeout)
        ));
    }

    #[test]
    fn oversized_protocol_frame_is_discarded_without_losing_the_next_heartbeat() {
        let mut input = vec![b'x'; 4_096];
        input.extend_from_slice(b"\nMINHA_UI_HEARTBEAT v1\r\n");

        let receiver = heartbeat_reader(Cursor::new(input));

        assert_eq!(receiver.recv_timeout(Duration::from_secs(1)), Ok(()));
    }

    #[cfg(unix)]
    #[test]
    fn unresponsive_child_is_terminated_after_the_startup_deadline() {
        let child = Command::new("sh")
            .args(["-c", "while :; do :; done"])
            .stdout(Stdio::piped())
            .spawn()
            .expect("spawn test child");
        let config = ProcessSupervisorConfig::production().with_timeouts(
            Duration::from_millis(25),
            Duration::from_millis(25),
            Duration::from_secs(1),
            Duration::from_millis(2),
        );

        let run = monitor_child(child, config).expect("monitor test child");

        assert_eq!(
            run,
            ChildRun::Unresponsive {
                healthy_window_elapsed: false
            }
        );
    }
}
