use core::ffi::c_void;
use std::fs;
use std::os::windows::process::CommandExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::ptr;
use std::sync::{Mutex, OnceLock};
use std::thread;
use std::time::{Duration, Instant};
use windows_registry::CURRENT_USER;

use crate::background_apps::{BackgroundAppEntry, safe_label};
use crate::native_event_route::NativeWindowId;
use crate::native_tray::NativeTrayIdentity;
use crate::tray_activation::CONTEXT_FIRST_VERSION;
use crate::tray_bridge_catalog::{TrayBridgeCatalog, TrayBridgeIdentity, TrayBridgeRecord};

const TRAY_BRIDGE_DLL_BYTES: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/obsidian_tray_bridge.dll"));
const TRAY_BRIDGE_HOST_BYTES: &[u8] =
    include_bytes!(concat!(env!("OUT_DIR"), "/obsidian_tray_bridge_host.exe"));

const SHARED_MAGIC: u32 = 0x4742_544f;
const PROTOCOL_VERSION: u16 = 2;
const RECORD_CAPACITY: usize = 128;
const TOOLTIP_UNITS: usize = 128;
const INSTALL_RETRY_DELAY: Duration = Duration::from_secs(5);
const HOST_START_TIMEOUT: Duration = Duration::from_secs(1);
const HOST_START_POLL: Duration = Duration::from_millis(20);
const MAPPING_NAME: &str = r"Local\ObsidianGlass.TrayBridge.Snapshot.v2";
const RUN_KEY: &str = r"Software\Microsoft\Windows\CurrentVersion\Run";
const RUN_VALUE: &str = "ObsidianGlassTrayBridge";

const FILE_MAP_READ: u32 = 0x0004;
const SYNCHRONIZE: u32 = 0x0010_0000;
const WAIT_TIMEOUT: u32 = 0x0000_0102;
const CREATE_NEW_PROCESS_GROUP: u32 = 0x0000_0200;
const CREATE_NO_WINDOW: u32 = 0x0800_0000;
const SECURITY_HEALTH_CALLBACK_MESSAGE: u32 = 0x0460;

#[link(name = "kernel32")]
unsafe extern "system" {
    fn CloseHandle(handle: *mut c_void) -> i32;
    fn OpenFileMappingW(desired_access: u32, inherit_handle: i32, name: *const u16) -> *mut c_void;
    fn MapViewOfFile(
        mapping: *mut c_void,
        desired_access: u32,
        offset_high: u32,
        offset_low: u32,
        bytes: usize,
    ) -> *mut c_void;
    fn OpenProcess(desired_access: u32, inherit_handle: i32, process_id: u32) -> *mut c_void;
    fn UnmapViewOfFile(address: *const c_void) -> i32;
    fn WaitForSingleObject(handle: *mut c_void, milliseconds: u32) -> u32;
}

#[link(name = "user32")]
unsafe extern "system" {
    fn GetWindowThreadProcessId(window: *mut c_void, process_id: *mut u32) -> u32;
}

#[derive(Clone, Copy)]
#[repr(C, align(8))]
struct WireRecord {
    committed_sequence: u64,
    active: u32,
    flags: u32,
    owner_window: u64,
    icon_id: u32,
    callback_message: u32,
    version: u32,
    has_guid: u32,
    guid: [u8; 16],
    tooltip: [u16; TOOLTIP_UNITS],
}

#[repr(C, align(8))]
struct WireSharedState {
    magic: u32,
    protocol_version: u16,
    record_size: u16,
    capacity: u32,
    reserved: u32,
    snapshot_sequence: u64,
    host_process_id: u32,
    explorer_thread_id: u32,
    records: [WireRecord; RECORD_CAPACITY],
}

impl Default for WireRecord {
    fn default() -> Self {
        Self {
            committed_sequence: 0,
            active: 0,
            flags: 0,
            owner_window: 0,
            icon_id: 0,
            callback_message: 0,
            version: 0,
            has_guid: 0,
            guid: [0; 16],
            tooltip: [0; TOOLTIP_UNITS],
        }
    }
}

#[cfg(test)]
impl WireRecord {
    fn test(
        committed_sequence: u64,
        active: u32,
        owner_window: u64,
        icon_id: u32,
        callback_message: u32,
    ) -> Self {
        Self {
            committed_sequence,
            active,
            owner_window,
            icon_id,
            callback_message,
            ..Self::default()
        }
    }
}

struct WireSnapshot {
    records: Vec<WireRecord>,
}

#[derive(Debug, thiserror::Error)]
enum TrayBridgeError {
    #[error("tray bridge payload cannot be written")]
    PayloadWrite,
    #[error("tray bridge host cannot be started")]
    HostStart,
    #[error("tray bridge host cannot be registered for logon")]
    HostRegistration,
    #[error("tray bridge shared snapshot is unavailable")]
    SharedSnapshot,
    #[error("tray bridge shared snapshot protocol is incompatible")]
    Protocol,
}

impl TrayBridgeError {
    const fn code(&self) -> &'static str {
        match self {
            Self::PayloadWrite => "payload_write",
            Self::HostStart => "host_start",
            Self::HostRegistration => "host_registration",
            Self::SharedSnapshot => "shared_snapshot",
            Self::Protocol => "protocol",
        }
    }
}

struct TrayBridgeRuntime {
    mapping: usize,
    view: usize,
    host_process_id: u32,
}

struct TrayBridgeHost {
    runtime: Option<TrayBridgeRuntime>,
    last_failed_install: Option<Instant>,
}

impl TrayBridgeHost {
    const fn new() -> Self {
        Self {
            runtime: None,
            last_failed_install: None,
        }
    }

    fn capture(&mut self, generation: u64) -> Vec<BackgroundAppEntry> {
        if self
            .runtime
            .as_ref()
            .is_some_and(|runtime| !runtime.host_is_alive())
        {
            self.runtime = None;
        }
        if self.runtime.is_none()
            && self
                .last_failed_install
                .is_none_or(|last| last.elapsed() >= INSTALL_RETRY_DELAY)
        {
            match TrayBridgeRuntime::install() {
                Ok(runtime) => {
                    self.runtime = Some(runtime);
                    self.last_failed_install = None;
                    record_bridge_diagnostic("tray_bridge.host_attached", "ok");
                }
                Err(error) => {
                    self.last_failed_install = Some(Instant::now());
                    record_bridge_diagnostic("tray_bridge.host_unavailable", error.code());
                }
            }
        }
        self.runtime
            .as_mut()
            .map_or_else(Vec::new, |runtime| runtime.capture(generation))
    }
}

impl TrayBridgeRuntime {
    fn install() -> Result<Self, TrayBridgeError> {
        let paths = write_embedded_bridge_host()?;
        register_host_startup(&paths.host)?;

        if let Ok(runtime) = Self::open() {
            return Ok(runtime);
        }

        Command::new(&paths.host)
            .current_dir(&paths.directory)
            .creation_flags(CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW)
            .spawn()
            .map_err(|_| TrayBridgeError::HostStart)?;

        let deadline = Instant::now() + HOST_START_TIMEOUT;
        loop {
            if let Ok(runtime) = Self::open() {
                return Ok(runtime);
            }
            if Instant::now() >= deadline {
                return Err(TrayBridgeError::SharedSnapshot);
            }
            thread::sleep(HOST_START_POLL);
        }
    }

    fn open() -> Result<Self, TrayBridgeError> {
        let mapping_name = wide_null(MAPPING_NAME);
        // SAFETY: The stable, session-local name is null terminated and the
        // returned read-only handle transfers immediately to the guard.
        let mapping = unsafe { OpenFileMappingW(FILE_MAP_READ, 0, mapping_name.as_ptr()) };
        if mapping.is_null() {
            return Err(TrayBridgeError::SharedSnapshot);
        }
        let mapping_guard = RawHandleGuard(mapping);
        // SAFETY: The successful mapping handle remains owned by the guard while
        // the fixed-size read-only view is validated.
        let view =
            unsafe { MapViewOfFile(mapping, FILE_MAP_READ, 0, 0, size_of::<WireSharedState>()) };
        if view.is_null() {
            return Err(TrayBridgeError::SharedSnapshot);
        }
        let view_guard = MappedViewGuard(view);
        // SAFETY: The view is exactly WireSharedState bytes and only immutable
        // protocol fields initialized by the host are read here.
        let state = unsafe { &*view.cast::<WireSharedState>() };
        if state.magic != SHARED_MAGIC
            || state.protocol_version != PROTOCOL_VERSION
            || usize::from(state.record_size) != size_of::<WireRecord>()
            || state.capacity as usize != RECORD_CAPACITY
            || state.host_process_id == 0
        {
            return Err(TrayBridgeError::Protocol);
        }
        let host_process_id = state.host_process_id;
        Ok(Self {
            mapping: mapping_guard.release() as usize,
            view: view_guard.release() as usize,
            host_process_id,
        })
    }

    fn host_is_alive(&self) -> bool {
        // SAFETY: SYNCHRONIZE grants no process-memory access. The copied PID
        // came from the validated host-owned snapshot header.
        let process = unsafe { OpenProcess(SYNCHRONIZE, 0, self.host_process_id) };
        if process.is_null() {
            return false;
        }
        let process = RawHandleGuard(process);
        // SAFETY: A zero-time wait only probes whether the process is signaled.
        unsafe { WaitForSingleObject(process.0, 0) == WAIT_TIMEOUT }
    }

    fn capture(&mut self, generation: u64) -> Vec<BackgroundAppEntry> {
        let state = self.view as *const WireSharedState;
        // SAFETY: The host owns the mapped state for this runtime's lifetime.
        // Writers publish each slot's committed sequence last; unstable copies
        // are marked inactive and excluded by the pure snapshot collector.
        let records = unsafe {
            let mut records = Vec::with_capacity(RECORD_CAPACITY);
            for index in 0..RECORD_CAPACITY {
                let source = ptr::addr_of!((*state).records[index]);
                let before = ptr::read_volatile(ptr::addr_of!((*source).committed_sequence));
                let mut record = ptr::read_volatile(source);
                let after = ptr::read_volatile(ptr::addr_of!((*source).committed_sequence));
                if before == 0 || before != after {
                    record.active = 0;
                }
                records.push(record);
            }
            records
        };
        let snapshot = collect_committed_records(&records);
        let mut catalog = TrayBridgeCatalog::new();
        for record in snapshot.records {
            if let Some(record) = decode_wire_record(record) {
                catalog.apply(record);
            }
        }
        catalog
            .records()
            .iter()
            .filter_map(|record| bridge_entry(record, generation))
            .collect()
    }
}

impl Drop for TrayBridgeRuntime {
    fn drop(&mut self) {
        // SAFETY: Both raw values were released from unique guards after a
        // successful attach and are released exactly once here.
        unsafe {
            let _ = UnmapViewOfFile(self.view as *const c_void);
            let _ = CloseHandle(self.mapping as *mut c_void);
        }
    }
}

struct RawHandleGuard(*mut c_void);
struct MappedViewGuard(*mut c_void);

impl RawHandleGuard {
    fn release(self) -> *mut c_void {
        let value = self.0;
        std::mem::forget(self);
        value
    }
}

impl MappedViewGuard {
    fn release(self) -> *mut c_void {
        let value = self.0;
        std::mem::forget(self);
        value
    }
}

impl Drop for RawHandleGuard {
    fn drop(&mut self) {
        // SAFETY: The guard exclusively owns a successful Win32 handle.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

impl Drop for MappedViewGuard {
    fn drop(&mut self) {
        // SAFETY: The guard exclusively owns a successful mapped view.
        let _ = unsafe { UnmapViewOfFile(self.0) };
    }
}

static TRAY_BRIDGE: OnceLock<Mutex<TrayBridgeHost>> = OnceLock::new();

pub(super) fn capture_tray_bridge_apps(generation: u64) -> Vec<BackgroundAppEntry> {
    TRAY_BRIDGE
        .get_or_init(|| Mutex::new(TrayBridgeHost::new()))
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .capture(generation)
}

fn decode_wire_record(record: WireRecord) -> Option<TrayBridgeRecord> {
    if record.active == 0 || record.owner_window == 0 {
        return None;
    }
    let tooltip_length = record
        .tooltip
        .iter()
        .position(|unit| *unit == 0)
        .unwrap_or(record.tooltip.len());
    Some(TrayBridgeRecord::new(
        TrayBridgeIdentity {
            owner_window: isize::try_from(record.owner_window).ok()?,
            icon_id: record.icon_id,
            guid: (record.has_guid != 0).then_some(record.guid),
        },
        record.callback_message,
        record.version,
        String::from_utf16_lossy(&record.tooltip[..tooltip_length]),
    ))
}

fn bridge_entry(record: &TrayBridgeRecord, generation: u64) -> Option<BackgroundAppEntry> {
    let window = record.owner_window() as *mut c_void;
    if window.is_null() {
        return None;
    }
    let mut process_id = 0_u32;
    // SAFETY: The copied owner HWND is revalidated synchronously before its PID
    // is used to resolve the executable path.
    let thread_id = unsafe { GetWindowThreadProcessId(window, &raw mut process_id) };
    if thread_id == 0 || process_id == 0 {
        return None;
    }
    let executable = crate::win32_background_apps::process_image_path(process_id)?;
    bridge_entry_from_resolved_owner(record, generation, process_id, &executable)
}

fn recovered_callback_message(executable: &str) -> Option<u32> {
    Path::new(executable)
        .file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name.eq_ignore_ascii_case("SecurityHealthSystray.exe"))
        .then_some(SECURITY_HEALTH_CALLBACK_MESSAGE)
}

fn bridge_entry_from_resolved_owner(
    record: &TrayBridgeRecord,
    generation: u64,
    process_id: u32,
    executable: &str,
) -> Option<BackgroundAppEntry> {
    let label = safe_label(record.tooltip(), executable, "");
    let (callback_message, version) = if record.callback_message() == 0 {
        (
            recovered_callback_message(executable)?,
            CONTEXT_FIRST_VERSION,
        )
    } else {
        (record.callback_message(), record.version())
    };
    let identity = NativeTrayIdentity::new(
        NativeWindowId::new(record.owner_window()),
        process_id,
        record.icon_id(),
        callback_message,
        version,
        record.guid(),
        generation,
    )?;
    Some(BackgroundAppEntry::native(
        identity,
        label,
        executable.to_owned(),
        executable.to_owned(),
    ))
}

struct EmbeddedBridgePaths {
    directory: PathBuf,
    host: PathBuf,
}

fn write_embedded_bridge_host() -> Result<EmbeddedBridgePaths, TrayBridgeError> {
    let payload_hash = TRAY_BRIDGE_DLL_BYTES
        .iter()
        .chain(TRAY_BRIDGE_HOST_BYTES)
        .fold(0xcbf2_9ce4_8422_2325_u64, |hash, byte| {
            (hash ^ u64::from(*byte)).wrapping_mul(0x0000_0100_0000_01b3)
        });
    let local_app_data = std::env::var_os("LOCALAPPDATA").ok_or(TrayBridgeError::PayloadWrite)?;
    let directory = bridge_install_directory(Path::new(&local_app_data), payload_hash);
    fs::create_dir_all(&directory).map_err(|_| TrayBridgeError::PayloadWrite)?;
    let dll = directory.join("obsidian_tray_bridge.dll");
    let host = directory.join("obsidian_tray_bridge_host.exe");
    write_payload_if_changed(&dll, TRAY_BRIDGE_DLL_BYTES)?;
    write_payload_if_changed(&host, TRAY_BRIDGE_HOST_BYTES)?;
    Ok(EmbeddedBridgePaths { directory, host })
}

fn bridge_install_directory(local_app_data: &Path, payload_hash: u64) -> PathBuf {
    local_app_data
        .join("ObsidianGlass")
        .join("TrayBridge")
        .join(format!("v2-{payload_hash:016x}"))
}

fn startup_command(host: &Path) -> String {
    format!("\"{}\"", host.display())
}

fn register_host_startup(host: &Path) -> Result<(), TrayBridgeError> {
    let key = CURRENT_USER
        .create(RUN_KEY)
        .map_err(|_| TrayBridgeError::HostRegistration)?;
    key.set_string(RUN_VALUE, startup_command(host))
        .map_err(|_| TrayBridgeError::HostRegistration)
}

fn write_payload_if_changed(path: &Path, bytes: &[u8]) -> Result<(), TrayBridgeError> {
    if fs::read(path).ok().as_deref() != Some(bytes) {
        fs::write(path, bytes).map_err(|_| TrayBridgeError::PayloadWrite)?;
    }
    Ok(())
}

fn wide_null(value: &str) -> Vec<u16> {
    value.encode_utf16().chain([0]).collect()
}

fn record_bridge_diagnostic(event: &'static str, code: &'static str) {
    crate::diagnostics::record(
        crate::diagnostics::DiagnosticModule::AppLifecycle,
        crate::diagnostics::LogLevel::Info,
        event,
        &[("code", code)],
    );
}

fn collect_committed_records(records: &[WireRecord]) -> WireSnapshot {
    let mut committed = records
        .iter()
        .copied()
        .filter(|record| record.active != 0 && record.committed_sequence != 0)
        .collect::<Vec<_>>();
    committed.sort_unstable_by_key(|record| record.committed_sequence);
    WireSnapshot { records: committed }
}

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::{
        RECORD_CAPACITY, TRAY_BRIDGE_DLL_BYTES, TRAY_BRIDGE_HOST_BYTES, WireRecord,
        bridge_entry_from_resolved_owner, bridge_install_directory, collect_committed_records,
        startup_command,
    };
    use crate::background_apps::BackgroundAppOrigin;
    use crate::tray_activation::CONTEXT_FIRST_VERSION;
    use crate::tray_bridge_catalog::{TrayBridgeIdentity, TrayBridgeRecord};

    #[test]
    fn embedded_tray_bridge_payloads_are_windows_pe_images() {
        for payload in [TRAY_BRIDGE_DLL_BYTES, TRAY_BRIDGE_HOST_BYTES] {
            assert!(payload.len() > 1024);
            assert_eq!(&payload[..2], b"MZ");
        }
    }

    #[test]
    fn bridge_payload_is_persistent_and_has_a_quoted_logon_command() {
        let local_app_data = Path::new(r"C:\Users\Test\AppData\Local");
        let directory = bridge_install_directory(local_app_data, 0x1234);
        let host = directory.join("obsidian_tray_bridge_host.exe");

        assert_eq!(
            directory,
            PathBuf::from(
                r"C:\Users\Test\AppData\Local\ObsidianGlass\TrayBridge\v2-0000000000001234"
            )
        );
        assert_eq!(startup_command(&host), format!("\"{}\"", host.display()));
        assert!(!directory.starts_with(std::env::temp_dir()));
    }

    #[test]
    fn snapshot_reader_ignores_inactive_and_uncommitted_records() {
        let mut records = vec![WireRecord::default(); RECORD_CAPACITY];
        records[2] = WireRecord::test(9, 1, 0x1234, 7, 0x8001);
        records[3] = WireRecord::test(10, 0, 0x2222, 8, 0x8002);
        records[4] = WireRecord::test(0, 1, 0x3333, 9, 0x8003);

        let snapshot = collect_committed_records(&records);

        assert_eq!(snapshot.records.len(), 1);
        assert_eq!(snapshot.records[0].owner_window, 0x1234);
    }

    #[test]
    fn reader_restart_rehydrates_every_active_icon_without_registration_replay() {
        let mut records = vec![WireRecord::default(); RECORD_CAPACITY];
        records[3] = WireRecord::test(100, 1, 0x1234, 7, 0x8001);
        records[17] = WireRecord::test(101, 1, 0x5678, 9, 0x8002);

        let snapshot = collect_committed_records(&records);

        assert_eq!(snapshot.records.len(), 2);
        assert_eq!(
            snapshot
                .records
                .iter()
                .map(|record| record.owner_window)
                .collect::<Vec<_>>(),
            [0x1234, 0x5678]
        );
    }

    #[test]
    fn explorer_reconnect_prunes_dead_records_without_resetting_live_callbacks() {
        let host_source = include_str!("win32_tray_bridge_host.cpp");

        assert!(host_source.contains("PruneSnapshot(*state, explorer_thread_id);"));
        assert!(host_source.contains("PruneSnapshot(*state, 0);"));
        assert!(!host_source.contains("ResetSnapshot"));
        assert!(!host_source.contains("InterlockedExchange64(&state.snapshot_sequence, 0)"));
    }

    #[test]
    fn callbackless_defender_record_recovers_its_native_callback_only() {
        let record = TrayBridgeRecord::new(
            TrayBridgeIdentity {
                owner_window: 0x1234,
                icon_id: 100,
                guid: None,
            },
            0,
            0,
            "Segurança do Windows - Nenhuma ação necessária.",
        );

        let defender = bridge_entry_from_resolved_owner(
            &record,
            9,
            12896,
            r"C:\Windows\System32\SecurityHealthSystray.exe",
        )
        .expect("known callbackless application icon remains live");
        let BackgroundAppOrigin::Native(identity) = defender.origin() else {
            panic!("callbackless bridge icon must preserve its native owner");
        };
        assert_eq!(identity.callback_message(), 0x0460);
        assert_eq!(identity.version(), CONTEXT_FIRST_VERSION);
        assert_eq!(
            defender.label(),
            "Segurança do Windows - Nenhuma ação necessária."
        );

        assert!(
            bridge_entry_from_resolved_owner(&record, 9, 8152, r"C:\Windows\explorer.exe",)
                .is_none()
        );
        assert!(
            bridge_entry_from_resolved_owner(&record, 9, 8153, r"C:\Apps\UnknownTray.exe",)
                .is_none()
        );
    }
}
