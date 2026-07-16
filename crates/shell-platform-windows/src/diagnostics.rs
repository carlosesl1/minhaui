use std::fs::{self, File, OpenOptions};
use std::io::{self, BufWriter, Write};
use std::path::{Path, PathBuf};
#[cfg(not(test))]
use std::sync::LazyLock;
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::{
    Receiver, RecvTimeoutError, Sender, SyncSender, TrySendError, channel, sync_channel,
};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use shell_diagnostics::{RetentionPolicy, STANDARD_DIAGNOSTIC_POLICY};

const DIAGNOSTIC_QUEUE_CAPACITY: usize = 256;
#[cfg(not(test))]
const DIAGNOSTIC_MODULES_ENV: &str = "MINHA_UI_LOG_MODULES";
const SLOW_DOCK_OPERATION: Duration = Duration::from_millis(16);

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum DiagnosticModule {
    AppLifecycle,
    DockLaunch,
    DockPerformance,
}

impl DiagnosticModule {
    const fn bit(self) -> u8 {
        match self {
            Self::AppLifecycle => 1 << 0,
            Self::DockLaunch => 1 << 1,
            Self::DockPerformance => 1 << 2,
        }
    }

    const fn as_str(self) -> &'static str {
        match self {
            Self::AppLifecycle => "app.lifecycle",
            Self::DockLaunch => "dock.launch",
            Self::DockPerformance => "dock.performance",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum LogLevel {
    Info,
    Error,
}

impl LogLevel {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Info => "INFO",
            Self::Error => "ERROR",
        }
    }
}

#[derive(Clone, Copy)]
struct EnabledModules(u8);

impl EnabledModules {
    const ALL: Self = Self(
        DiagnosticModule::AppLifecycle.bit()
            | DiagnosticModule::DockLaunch.bit()
            | DiagnosticModule::DockPerformance.bit(),
    );

    fn from_spec(spec: Option<&str>) -> Self {
        let Some(spec) = spec else {
            return Self(0);
        };
        let mut mask = 0;
        for module in spec.split(',').map(str::trim) {
            match module {
                "all" => return Self::ALL,
                "app.lifecycle" => mask |= DiagnosticModule::AppLifecycle.bit(),
                "dock.launch" => mask |= DiagnosticModule::DockLaunch.bit(),
                "dock.performance" => mask |= DiagnosticModule::DockPerformance.bit(),
                "" | "none" | "off" => {}
                _ => {}
            }
        }
        Self(mask)
    }

    const fn contains(self, module: DiagnosticModule) -> bool {
        self.0 & module.bit() != 0
    }

    const fn is_empty(self) -> bool {
        self.0 == 0
    }
}

struct DiagnosticField {
    key: &'static str,
    value: String,
}

struct DiagnosticEvent {
    timestamp_ms: u128,
    queued_at: Instant,
    module: DiagnosticModule,
    level: LogLevel,
    event: &'static str,
    fields: Vec<DiagnosticField>,
    dropped_before: u64,
}

impl DiagnosticEvent {
    fn new(
        module: DiagnosticModule,
        level: LogLevel,
        event: &'static str,
        fields: &[(&'static str, &str)],
    ) -> Self {
        Self {
            timestamp_ms: timestamp_ms(),
            queued_at: Instant::now(),
            module,
            level,
            event,
            fields: fields
                .iter()
                .take(STANDARD_DIAGNOSTIC_POLICY.max_fields())
                .map(|(key, value)| DiagnosticField {
                    key,
                    value: STANDARD_DIAGNOSTIC_POLICY.sanitize_value(key, value),
                })
                .collect(),
            dropped_before: 0,
        }
    }

    #[cfg(test)]
    fn test(event: &'static str) -> Self {
        Self::new(DiagnosticModule::DockLaunch, LogLevel::Info, event, &[])
    }
}

enum ControlCommand {
    Shutdown(SyncSender<()>),
}

#[derive(Debug, Eq, PartialEq)]
enum EnqueueOutcome {
    Queued,
    DroppedFull,
    Disconnected,
}

struct Diagnostics {
    enabled: EnabledModules,
    sender: SyncSender<DiagnosticEvent>,
    control: Sender<ControlCommand>,
    worker: Mutex<Option<JoinHandle<()>>>,
    dropped_events: AtomicU64,
}

impl Diagnostics {
    fn start(path: PathBuf, enabled: EnabledModules, capacity: usize) -> io::Result<Self> {
        let (sender, receiver) = sync_channel(capacity);
        let (control, control_receiver) = channel();
        let worker = thread::Builder::new()
            .name("minha-ui-diagnostics".to_owned())
            .spawn(move || run_writer(&path, receiver, control_receiver))?;
        Ok(Self {
            enabled,
            sender,
            control,
            worker: Mutex::new(Some(worker)),
            dropped_events: AtomicU64::new(0),
        })
    }

    fn record(
        &self,
        module: DiagnosticModule,
        level: LogLevel,
        event: &'static str,
        fields: &[(&'static str, &str)],
    ) {
        if !self.enabled.contains(module) {
            return;
        }
        let dropped_before = self.dropped_events.swap(0, Ordering::Relaxed);
        let mut diagnostic_event = DiagnosticEvent::new(module, level, event, fields);
        diagnostic_event.dropped_before = dropped_before;
        if try_enqueue(&self.sender, diagnostic_event) != EnqueueOutcome::Queued {
            self.dropped_events
                .fetch_add(dropped_before.saturating_add(1), Ordering::Relaxed);
        }
    }

    fn shutdown(&self) {
        let (acknowledge, acknowledged) = sync_channel(1);
        if self
            .control
            .send(ControlCommand::Shutdown(acknowledge))
            .is_err()
        {
            return;
        }
        if acknowledged
            .recv_timeout(Duration::from_millis(250))
            .is_err()
        {
            return;
        }
        if let Ok(mut worker) = self.worker.lock()
            && let Some(handle) = worker.take()
        {
            let _ = handle.join();
        }
    }
}

#[cfg(not(test))]
static DIAGNOSTICS: LazyLock<Option<Diagnostics>> = LazyLock::new(|| {
    let modules = std::env::var(DIAGNOSTIC_MODULES_ENV).ok();
    let enabled = EnabledModules::from_spec(modules.as_deref());
    if enabled.is_empty() {
        return None;
    }
    match Diagnostics::start(diagnostic_log_path(), enabled, DIAGNOSTIC_QUEUE_CAPACITY) {
        Ok(diagnostics) => Some(diagnostics),
        Err(error) => {
            eprintln!("diagnostic worker start failed: {error}");
            None
        }
    }
});

#[cfg(not(test))]
pub(crate) fn record(
    module: DiagnosticModule,
    level: LogLevel,
    event: &'static str,
    fields: &[(&'static str, &str)],
) {
    if let Some(diagnostics) = DIAGNOSTICS.as_ref() {
        diagnostics.record(module, level, event, fields);
    }
}

#[cfg(not(test))]
pub(crate) fn enabled(module: DiagnosticModule) -> bool {
    DIAGNOSTICS
        .as_ref()
        .is_some_and(|diagnostics| diagnostics.enabled.contains(module))
}

#[cfg(test)]
pub(crate) const fn enabled(_module: DiagnosticModule) -> bool {
    false
}

#[cfg(test)]
pub(crate) const fn record(
    _module: DiagnosticModule,
    _level: LogLevel,
    _event: &'static str,
    _fields: &[(&'static str, &str)],
) {
}

#[cfg(not(test))]
pub(crate) fn shutdown() {
    if let Some(diagnostics) = DIAGNOSTICS.as_ref() {
        diagnostics.shutdown();
    }
}

#[cfg(test)]
pub(crate) const fn shutdown() {}

fn try_enqueue(sender: &SyncSender<DiagnosticEvent>, event: DiagnosticEvent) -> EnqueueOutcome {
    match sender.try_send(event) {
        Ok(()) => EnqueueOutcome::Queued,
        Err(TrySendError::Full(_)) => EnqueueOutcome::DroppedFull,
        Err(TrySendError::Disconnected(_)) => EnqueueOutcome::Disconnected,
    }
}

#[cfg(not(test))]
fn diagnostic_log_path() -> PathBuf {
    std::env::var_os("LOCALAPPDATA")
        .map(PathBuf::from)
        .unwrap_or_else(std::env::temp_dir)
        .join("Minha UI")
        .join("logs")
        .join("shell-app.log")
}

fn run_writer(path: &Path, receiver: Receiver<DiagnosticEvent>, control: Receiver<ControlCommand>) {
    let mut writer = match BoundedLogWriter::open(path, STANDARD_DIAGNOSTIC_POLICY.retention()) {
        Ok(writer) => writer,
        Err(error) => {
            eprintln!("diagnostic log open failed: {error}");
            return;
        }
    };
    loop {
        if let Ok(ControlCommand::Shutdown(acknowledge)) = control.try_recv() {
            for event in receiver.try_iter().take(DIAGNOSTIC_QUEUE_CAPACITY) {
                if let Err(error) = writer.write_event(&event) {
                    eprintln!("diagnostic log write failed: {error}");
                    break;
                }
            }
            let _ = writer.flush();
            let _ = acknowledge.send(());
            return;
        }
        match receiver.recv_timeout(Duration::from_millis(25)) {
            Ok(event) => {
                if let Err(error) = writer.write_event(&event) {
                    eprintln!("diagnostic log write failed: {error}");
                    return;
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
    }
}

struct BoundedLogWriter {
    path: PathBuf,
    writer: BufWriter<File>,
    bytes_written: u64,
    retention: RetentionPolicy,
}

impl BoundedLogWriter {
    fn open(path: &Path, retention: RetentionPolicy) -> io::Result<Self> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let existing_bytes = fs::metadata(path).map_or(0, |metadata| metadata.len());
        let truncate = existing_bytes >= retention.max_file_bytes();
        let writer = open_log_file(path, truncate)?;
        Ok(Self {
            path: path.to_path_buf(),
            writer,
            bytes_written: if truncate { 0 } else { existing_bytes },
            retention,
        })
    }

    fn write_event(&mut self, event: &DiagnosticEvent) -> io::Result<()> {
        let mut line = Vec::with_capacity(256);
        write_record(&mut line, event)?;
        let line_bytes = u64::try_from(line.len()).unwrap_or(u64::MAX);
        if !self.retention.accepts_record(line_bytes) {
            return Ok(());
        }
        if self.retention.should_rotate(self.bytes_written, line_bytes) {
            self.writer.flush()?;
            self.writer = open_log_file(&self.path, true)?;
            self.bytes_written = 0;
        }
        self.writer.write_all(&line)?;
        self.writer.flush()?;
        self.bytes_written = self.bytes_written.saturating_add(line_bytes);
        Ok(())
    }

    fn flush(&mut self) -> io::Result<()> {
        self.writer.flush()
    }
}

fn open_log_file(path: &Path, truncate: bool) -> io::Result<BufWriter<File>> {
    OpenOptions::new()
        .create(true)
        .write(true)
        .append(!truncate)
        .truncate(truncate)
        .open(path)
        .map(BufWriter::new)
}

fn write_record(writer: &mut impl Write, record: &DiagnosticEvent) -> io::Result<()> {
    write!(
        writer,
        "timestamp_ms={} queue_delay_us={} level={} module={} event={} pid={}",
        record.timestamp_ms,
        record.queued_at.elapsed().as_micros(),
        record.level.as_str(),
        record.module.as_str(),
        escape_field(record.event),
        std::process::id()
    )?;
    for field in &record.fields {
        write!(
            writer,
            " {}=\"{}\"",
            escape_field(field.key),
            escape_field(&field.value)
        )?;
    }
    if record.dropped_before > 0 {
        write!(writer, " dropped_before={}", record.dropped_before)?;
    }
    writeln!(writer)
}

pub(crate) fn is_slow_dock_operation(duration: Duration) -> bool {
    duration > SLOW_DOCK_OPERATION
}

fn timestamp_ms() -> u128 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |duration| duration.as_millis())
}

fn escape_field(value: &str) -> String {
    value.chars().flat_map(char::escape_default).collect()
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::io;
    use std::path::PathBuf;
    use std::sync::mpsc::{channel, sync_channel};
    use std::time::{Duration, SystemTime, UNIX_EPOCH};

    use shell_diagnostics::{RetentionPolicy, STANDARD_DIAGNOSTIC_POLICY};

    use super::{
        BoundedLogWriter, ControlCommand, DiagnosticEvent, DiagnosticModule, Diagnostics,
        EnabledModules, EnqueueOutcome, LogLevel, try_enqueue,
    };

    #[test]
    fn module_filter_enables_only_the_requested_diagnostics() {
        let enabled = EnabledModules::from_spec(Some("dock.launch"));

        assert!(enabled.contains(DiagnosticModule::DockLaunch));
        assert!(!enabled.contains(DiagnosticModule::AppLifecycle));
        let performance = EnabledModules::from_spec(Some("dock.performance"));
        assert!(performance.contains(DiagnosticModule::DockPerformance));
    }

    #[test]
    fn diagnostics_are_disabled_until_a_module_is_requested() {
        let enabled = EnabledModules::from_spec(None);

        assert!(enabled.is_empty());
        assert!(!enabled.contains(DiagnosticModule::DockLaunch));
        assert!(!enabled.contains(DiagnosticModule::AppLifecycle));
    }

    #[test]
    fn diagnostic_values_never_keep_raw_control_characters() {
        let escaped = super::escape_field("value\0\t\u{1b}");

        assert!(!escaped.chars().any(char::is_control));
    }

    #[test]
    fn diagnostic_values_redact_user_profile_paths() {
        let event = DiagnosticEvent::new(
            DiagnosticModule::AppLifecycle,
            LogLevel::Error,
            "app.runtime.failed",
            &[("error", r"failed at C:\Users\Carlos\Private\file")],
        );

        assert_eq!(event.fields[0].value, "[redacted]");
    }

    #[test]
    fn diagnostic_fields_are_bounded_before_enqueue() {
        let oversized = "x".repeat(STANDARD_DIAGNOSTIC_POLICY.max_value_chars() * 2);
        let event = DiagnosticEvent::new(
            DiagnosticModule::DockLaunch,
            LogLevel::Info,
            "dock.launch.failed",
            &[("error", &oversized)],
        );

        assert_eq!(
            event.fields[0].value.chars().count(),
            STANDARD_DIAGNOSTIC_POLICY.max_value_chars()
        );
    }

    #[test]
    fn dock_performance_logs_only_operations_over_the_frame_budget() {
        assert!(!super::is_slow_dock_operation(Duration::from_millis(16)));
        assert!(super::is_slow_dock_operation(Duration::from_millis(17)));
    }

    #[test]
    fn full_diagnostic_queue_drops_instead_of_waiting() {
        let (sender, _receiver) = sync_channel(1);

        assert_eq!(
            try_enqueue(&sender, DiagnosticEvent::test("dock.launch.first")),
            EnqueueOutcome::Queued
        );
        assert_eq!(
            try_enqueue(&sender, DiagnosticEvent::test("dock.launch.second")),
            EnqueueOutcome::DroppedFull
        );
    }

    #[test]
    fn shutdown_control_bypasses_a_full_event_queue() -> io::Result<()> {
        let (event_sender, _event_receiver) = sync_channel(1);
        assert_eq!(
            try_enqueue(&event_sender, DiagnosticEvent::test("dock.launch.first")),
            EnqueueOutcome::Queued
        );
        let (control_sender, control_receiver) = channel();
        let (acknowledge, _acknowledged) = sync_channel(1);

        control_sender
            .send(ControlCommand::Shutdown(acknowledge))
            .map_err(io::Error::other)?;

        assert!(matches!(
            control_receiver.try_recv(),
            Ok(ControlCommand::Shutdown(_))
        ));
        Ok(())
    }

    #[test]
    fn worker_persists_only_enabled_modules_as_structured_lines() -> io::Result<()> {
        let path = std::env::temp_dir().join(format!(
            "minha-ui-diagnostics-{}-{}.log",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |duration| duration.as_nanos())
        ));
        let diagnostics = Diagnostics::start(
            path.clone(),
            EnabledModules::from_spec(Some("app.lifecycle")),
            8,
        )?;

        diagnostics.record(
            DiagnosticModule::DockLaunch,
            LogLevel::Info,
            "dock.launch.succeeded",
            &[("target", "taskmgr.exe")],
        );
        diagnostics.record(
            DiagnosticModule::AppLifecycle,
            LogLevel::Error,
            "app.runtime.failed",
            &[("error", "failed\nwith details")],
        );
        diagnostics.shutdown();

        let contents = fs::read_to_string(&path)?;
        assert!(contents.contains("module=app.lifecycle event=app.runtime.failed"));
        assert!(contents.contains("error=\"failed\\nwith details\""));
        assert!(!contents.contains("dock.launch.succeeded"));
        assert_eq!(contents.lines().count(), 1);
        fs::remove_file(path)?;
        Ok(())
    }

    #[test]
    fn diagnostic_log_is_truncated_before_it_exceeds_its_budget() -> io::Result<()> {
        let path = temporary_log_path();
        let mut writer = BoundedLogWriter::open(&path, RetentionPolicy::new(1, 128))?;
        let event = DiagnosticEvent::new(
            DiagnosticModule::DockLaunch,
            LogLevel::Error,
            "dock.launch.failed",
            &[("error", &"x".repeat(256))],
        );

        for _ in 0..8 {
            writer.write_event(&event)?;
        }
        drop(writer);

        assert!(fs::metadata(&path)?.len() <= 128);
        fs::remove_file(path)?;
        Ok(())
    }

    fn temporary_log_path() -> PathBuf {
        std::env::temp_dir().join(format!(
            "minha-ui-diagnostics-{}-{}.log",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |duration| duration.as_nanos())
        ))
    }
}
