use core::ffi::c_void;

use windows::Win32::Foundation::{CloseHandle, ERROR_ACCESS_DENIED, HANDLE, HWND, LPARAM, WPARAM};
use windows::Win32::System::Diagnostics::Debug::ReadProcessMemory;
use windows::Win32::System::Memory::{
    MEM_COMMIT, MEM_RELEASE, MEM_RESERVE, MEMORY_BASIC_INFORMATION, PAGE_EXECUTE_READ,
    PAGE_EXECUTE_READWRITE, PAGE_EXECUTE_WRITECOPY, PAGE_GUARD, PAGE_NOACCESS,
    PAGE_PROTECTION_FLAGS, PAGE_READONLY, PAGE_READWRITE, PAGE_WRITECOPY, VirtualAllocEx,
    VirtualFreeEx, VirtualQueryEx,
};
use windows::Win32::System::Threading::{
    GetProcessId, OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION,
    PROCESS_VM_OPERATION, PROCESS_VM_READ, PROCESS_VM_WRITE, QueryFullProcessImageNameW,
};
use windows::Win32::UI::Controls::{TB_BUTTONCOUNT, TB_GETBUTTON};
use windows::Win32::UI::WindowsAndMessaging::{
    FindWindowExW, FindWindowW, GetWindowThreadProcessId, IsWindow, SMTO_ABORTIFHUNG,
    SendMessageTimeoutW,
};
use windows::core::{HRESULT, PWSTR, w};

use crate::NativeWindowId;
use crate::background_apps::{BackgroundAppEntry, safe_label};
use crate::native_tray::NativeTrayIdentity;
use crate::tray_record_decoder::{
    MAX_NATIVE_TRAY_CANDIDATES, MAX_TRAY_TOOLTIP_CHARS, bounded_candidate_count,
    decode_bounded_tooltip, decode_x64_button, decode_x64_tray_data, validate_owner_process_id,
};

const X64_TRAY_DATA_BYTES: usize = 32;
const X64_TBBUTTON_BYTES: usize = 32;
const BOUNDED_TOOLTIP_BYTES: usize = (MAX_TRAY_TOOLTIP_CHARS + 1) * size_of::<u16>();
const TOOLBAR_MESSAGE_TIMEOUT_MS: u32 = 250;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TrayHost {
    Notification,
    Overflow,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub(super) enum NativeTrayCaptureError {
    #[error("native tray layout unsupported")]
    Unsupported,
    #[error("native tray access denied")]
    AccessDenied,
    #[error("native tray host unavailable")]
    HostUnavailable,
    #[error("native tray toolbar unavailable")]
    ToolbarUnavailable,
    #[error("native tray process memory unavailable")]
    ProcessMemoryUnavailable,
}

impl NativeTrayCaptureError {
    pub(super) const fn code(self) -> &'static str {
        match self {
            Self::Unsupported => "native_tray_unsupported",
            Self::AccessDenied => "native_tray_access_denied",
            Self::HostUnavailable => "native_tray_host_unavailable",
            Self::ToolbarUnavailable => "native_tray_toolbar_unavailable",
            Self::ProcessMemoryUnavailable => "native_tray_process_memory_unavailable",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum NativeTrayCaptureOutcome {
    Complete,
    Partial(NativeTrayCaptureError),
    Unavailable(NativeTrayCaptureError),
}

impl NativeTrayCaptureOutcome {
    pub(super) const fn code(self) -> &'static str {
        match self {
            Self::Complete => "native_tray_complete",
            Self::Partial(error) | Self::Unavailable(error) => error.code(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(super) struct NativeTrayCapture {
    pub(super) entries: Vec<BackgroundAppEntry>,
    pub(super) outcome: NativeTrayCaptureOutcome,
}

trait TrayToolbarReader {
    fn read_host(&mut self, host: TrayHost, generation: u64, remaining: usize) -> TrayHostCapture;
}

#[derive(Clone, Debug, Eq, PartialEq)]
struct TrayHostCapture {
    entries: Vec<BackgroundAppEntry>,
    raw_candidates: usize,
    error: Option<NativeTrayCaptureError>,
}

impl TrayHostCapture {
    fn complete(entries: Vec<BackgroundAppEntry>, raw_candidates: usize) -> Self {
        Self {
            entries,
            raw_candidates,
            error: None,
        }
    }

    fn failed(
        entries: Vec<BackgroundAppEntry>,
        raw_candidates: usize,
        error: NativeTrayCaptureError,
    ) -> Self {
        Self {
            entries,
            raw_candidates,
            error: Some(error),
        }
    }
}

trait ProcessMemoryReader {
    fn is_readable(&self, address: usize, byte_len: usize) -> bool;
    fn read_exact(&self, address: usize, destination: &mut [u8]) -> bool;
}

trait ToolbarSession: ProcessMemoryReader {
    fn button_count(&mut self) -> Result<usize, NativeTrayCaptureError>;
    fn read_button(
        &mut self,
        index: usize,
        destination: &mut [u8; 32],
    ) -> Result<(), NativeTrayCaptureError>;
}

struct ResolvedTrayOwner {
    process_id: u32,
    executable: String,
}

trait TrayOwnerResolver {
    fn resolve_owner(&self, owner_window: usize) -> Option<ResolvedTrayOwner>;
}

fn read_toolbar_session(
    session: &mut impl ToolbarSession,
    owners: &impl TrayOwnerResolver,
    generation: u64,
    remaining: usize,
) -> TrayHostCapture {
    let count = match session.button_count() {
        Ok(count) => bounded_candidate_count(count).min(remaining),
        Err(error) => return TrayHostCapture::failed(Vec::new(), 0, error),
    };
    let mut entries = Vec::with_capacity(count);
    for index in 0..count {
        let mut button = [0_u8; 32];
        if let Err(error) = session.read_button(index, &mut button) {
            return TrayHostCapture::failed(entries, index + 1, error);
        }
        if let Some(entry) = decode_toolbar_entry(session, owners, &button, generation) {
            entries.push(entry);
        }
    }
    TrayHostCapture::complete(entries, count)
}

fn decode_toolbar_entry(
    memory: &impl ProcessMemoryReader,
    owners: &impl TrayOwnerResolver,
    button: &[u8],
    generation: u64,
) -> Option<BackgroundAppEntry> {
    let decoded_button = decode_x64_button(button, 1..usize::MAX).ok()?;
    let tray_data_address = decoded_button.tray_data_pointer();
    if !memory.is_readable(tray_data_address, X64_TRAY_DATA_BYTES) {
        return None;
    }
    if let Some(span) = decoded_button.tooltip_span()
        && (span.byte_len() != BOUNDED_TOOLTIP_BYTES
            || !memory.is_readable(span.pointer(), span.byte_len()))
    {
        return None;
    }

    let mut tray_data = [0_u8; X64_TRAY_DATA_BYTES];
    if !memory.read_exact(tray_data_address, &mut tray_data) {
        return None;
    }
    let record = decode_x64_tray_data(decoded_button, tray_data_address, &tray_data).ok()?;
    let owner = owners.resolve_owner(record.owner_window())?;
    validate_owner_process_id(owner.process_id).ok()?;

    let tooltip = if let Some(span) = decoded_button.tooltip_span() {
        let mut tooltip_bytes = [0_u8; BOUNDED_TOOLTIP_BYTES];
        if !memory.read_exact(span.pointer(), &mut tooltip_bytes) {
            return None;
        }
        let mut tooltip_units = [0_u16; MAX_TRAY_TOOLTIP_CHARS + 1];
        for (unit, bytes) in tooltip_units
            .iter_mut()
            .zip(tooltip_bytes.chunks_exact(size_of::<u16>()))
        {
            *unit = u16::from_le_bytes([bytes[0], bytes[1]]);
        }
        decode_bounded_tooltip(&tooltip_units).ok()?
    } else {
        String::new()
    };
    let identity = NativeTrayIdentity::new(
        NativeWindowId::new(record.owner_window() as isize),
        owner.process_id,
        record.icon_id(),
        record.callback_message(),
        0,
        None,
        generation,
    )?;
    let label = safe_label(&tooltip, &owner.executable, "");
    Some(BackgroundAppEntry::native(
        identity,
        label,
        owner.executable.clone(),
        owner.executable,
    ))
}

struct ExplorerTraySource<R> {
    reader: R,
}

impl<R: TrayToolbarReader> ExplorerTraySource<R> {
    fn new(reader: R) -> Self {
        Self { reader }
    }

    fn capture(&mut self, generation: u64) -> NativeTrayCapture {
        let mut entries = Vec::new();
        let mut first_error = None;
        let mut successful_hosts = 0_u8;
        let mut raw_candidates_seen = 0_usize;

        for host in [TrayHost::Notification, TrayHost::Overflow] {
            let remaining = MAX_NATIVE_TRAY_CANDIDATES.saturating_sub(raw_candidates_seen);
            let mut host_capture = self.reader.read_host(host, generation, remaining);
            let raw_candidates = host_capture.raw_candidates.min(remaining);
            raw_candidates_seen += raw_candidates;
            host_capture.entries.truncate(raw_candidates);
            entries.extend(host_capture.entries);
            if let Some(error) = host_capture.error {
                first_error.get_or_insert(error);
            } else {
                successful_hosts += 1;
            }
        }

        let outcome = match (first_error, successful_hosts) {
            (None, _) => NativeTrayCaptureOutcome::Complete,
            (Some(error), 0) if entries.is_empty() => NativeTrayCaptureOutcome::Unavailable(error),
            (Some(error), _) => NativeTrayCaptureOutcome::Partial(error),
        };
        NativeTrayCapture { entries, outcome }
    }
}

struct Win32TrayToolbarReader {
    shell_window: HWND,
    explorer_process_id: u32,
}

impl Win32TrayToolbarReader {
    fn discover() -> Result<Self, NativeTrayCaptureError> {
        let shell_window = find_shell_tray_window()?;
        let explorer_process_id =
            window_process_id(shell_window).ok_or(NativeTrayCaptureError::HostUnavailable)?;
        Ok(Self {
            shell_window,
            explorer_process_id,
        })
    }
}

impl TrayToolbarReader for Win32TrayToolbarReader {
    fn read_host(&mut self, host: TrayHost, generation: u64, remaining: usize) -> TrayHostCapture {
        let toolbar = match locate_toolbar(host, self.shell_window) {
            Ok(toolbar) => toolbar,
            Err(error) => return TrayHostCapture::failed(Vec::new(), 0, error),
        };
        let toolbar_process_id = window_process_id(toolbar).unwrap_or(0);
        let explorer_process_id =
            match validate_toolbar_process_id(self.explorer_process_id, toolbar_process_id) {
                Ok(process_id) => process_id,
                Err(error) => return TrayHostCapture::failed(Vec::new(), 0, error),
            };
        let process = match OwnedProcess::open_toolbar(explorer_process_id) {
            Ok(process) => process,
            Err(error) => return TrayHostCapture::failed(Vec::new(), 0, error),
        };
        let remote_button = match RemoteButtonBuffer::allocate(process.handle()) {
            Ok(remote_button) => remote_button,
            Err(error) => return TrayHostCapture::failed(Vec::new(), 0, error),
        };
        let mut session = Win32ToolbarSession {
            remote_button,
            process,
            toolbar,
        };
        read_toolbar_session(&mut session, &Win32TrayOwnerResolver, generation, remaining)
    }
}

pub(super) fn capture_native_tray_apps(generation: u64) -> NativeTrayCapture {
    if usize::BITS != 64 {
        return NativeTrayCapture {
            entries: Vec::new(),
            outcome: NativeTrayCaptureOutcome::Unavailable(NativeTrayCaptureError::Unsupported),
        };
    }

    let reader = match Win32TrayToolbarReader::discover() {
        Ok(reader) => reader,
        Err(error) => {
            return NativeTrayCapture {
                entries: Vec::new(),
                outcome: NativeTrayCaptureOutcome::Unavailable(error),
            };
        }
    };
    ExplorerTraySource::new(reader).capture(generation)
}

fn find_shell_tray_window() -> Result<HWND, NativeTrayCaptureError> {
    // SAFETY: Category 8 (FFI boundary). This read-only class-name lookup
    // returns one copied HWND used as the Explorer trust anchor.
    unsafe { FindWindowW(w!("Shell_TrayWnd"), None) }
        .map_err(|_| NativeTrayCaptureError::HostUnavailable)
}

fn locate_toolbar(host: TrayHost, shell_window: HWND) -> Result<HWND, NativeTrayCaptureError> {
    match host {
        TrayHost::Notification => {
            // SAFETY: Category 8 (FFI boundary). The parent is the live Explorer
            // shell trust anchor resolved once for this capture.
            let tray =
                unsafe { FindWindowExW(Some(shell_window), None, w!("TrayNotifyWnd"), None) }
                    .map_err(|_| NativeTrayCaptureError::HostUnavailable)?;
            // SAFETY: Category 8 (FFI boundary). The lookup is scoped to the
            // Explorer notification-area window.
            let pager = unsafe { FindWindowExW(Some(tray), None, w!("SysPager"), None) }
                .map_err(|_| NativeTrayCaptureError::HostUnavailable)?;
            // SAFETY: Category 8 (FFI boundary). Only Explorer's toolbar child
            // is returned; owner application windows are never messaged.
            unsafe { FindWindowExW(Some(pager), None, w!("ToolbarWindow32"), None) }
                .map_err(|_| NativeTrayCaptureError::HostUnavailable)
        }
        TrayHost::Overflow => {
            // SAFETY: Category 8 (FFI boundary). This read-only lookup returns
            // Explorer's overflow host HWND without retaining Rust memory.
            let overflow = unsafe { FindWindowW(w!("NotifyIconOverflowWindow"), None) }
                .map_err(|_| NativeTrayCaptureError::HostUnavailable)?;
            // SAFETY: Category 8 (FFI boundary). Only the toolbar child of the
            // resolved Explorer overflow host is returned.
            unsafe { FindWindowExW(Some(overflow), None, w!("ToolbarWindow32"), None) }
                .map_err(|_| NativeTrayCaptureError::HostUnavailable)
        }
    }
}

fn window_process_id(window: HWND) -> Option<u32> {
    let mut process_id = 0_u32;
    // SAFETY: Category 8 (FFI boundary). `process_id` is initialized writable
    // storage and the HWND is a copied identity from the immediate lookup.
    let thread_id = unsafe { GetWindowThreadProcessId(window, Some(&mut process_id)) };
    (thread_id != 0 && process_id != 0).then_some(process_id)
}

const fn validate_toolbar_process_id(
    trusted_explorer_process_id: u32,
    toolbar_process_id: u32,
) -> Result<u32, NativeTrayCaptureError> {
    if trusted_explorer_process_id != 0
        && toolbar_process_id != 0
        && toolbar_process_id == trusted_explorer_process_id
    {
        Ok(toolbar_process_id)
    } else {
        Err(NativeTrayCaptureError::HostUnavailable)
    }
}

struct OwnedProcess(HANDLE);

impl OwnedProcess {
    fn open_toolbar(process_id: u32) -> Result<Self, NativeTrayCaptureError> {
        let access = PROCESS_QUERY_LIMITED_INFORMATION
            | PROCESS_VM_OPERATION
            | PROCESS_VM_READ
            | PROCESS_VM_WRITE;
        // SAFETY: Category 8 (FFI boundary). The requested rights are limited to
        // image query and the documented cross-process toolbar buffer workflow.
        unsafe { OpenProcess(access, false, process_id) }
            .map(Self)
            .map_err(map_open_process_error)
    }

    fn open_owner(process_id: u32) -> Option<Self> {
        // SAFETY: Category 8 (FFI boundary). Owner resolution requests only the
        // minimal right required to query the current executable path.
        unsafe { OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, process_id) }
            .ok()
            .map(Self)
    }

    const fn handle(&self) -> HANDLE {
        self.0
    }
}

impl Drop for OwnedProcess {
    fn drop(&mut self) {
        // SAFETY: Category 2 (resource cleanup). This guard exclusively owns a
        // successful OpenProcess handle and closes it exactly once.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

fn map_open_process_error(error: windows::core::Error) -> NativeTrayCaptureError {
    if error.code() == HRESULT::from_win32(ERROR_ACCESS_DENIED.0) {
        NativeTrayCaptureError::AccessDenied
    } else {
        NativeTrayCaptureError::ProcessMemoryUnavailable
    }
}

struct RemoteButtonBuffer {
    process: HANDLE,
    address: *mut c_void,
}

impl RemoteButtonBuffer {
    fn allocate(process: HANDLE) -> Result<Self, NativeTrayCaptureError> {
        // SAFETY: Category 8 (FFI boundary). The allocation is a single fixed
        // x64 TBBUTTON buffer in the opened Explorer process and is immediately
        // transferred to this exactly-once release guard.
        let address = unsafe {
            VirtualAllocEx(
                process,
                None,
                X64_TBBUTTON_BYTES,
                MEM_COMMIT | MEM_RESERVE,
                PAGE_READWRITE,
            )
        };
        if address.is_null() {
            Err(NativeTrayCaptureError::ProcessMemoryUnavailable)
        } else {
            Ok(Self { process, address })
        }
    }

    fn address(&self) -> usize {
        self.address as usize
    }
}

impl Drop for RemoteButtonBuffer {
    fn drop(&mut self) {
        // SAFETY: Category 2 (resource cleanup). The process handle remains live
        // because Win32ToolbarSession declares this guard before its owner, and
        // MEM_RELEASE requires a zero size for the exact allocation base.
        let _ = unsafe { VirtualFreeEx(self.process, self.address, 0, MEM_RELEASE) };
    }
}

struct Win32ToolbarSession {
    remote_button: RemoteButtonBuffer,
    process: OwnedProcess,
    toolbar: HWND,
}

impl Win32ToolbarSession {
    fn toolbar_message(
        &self,
        message: u32,
        wparam: WPARAM,
        lparam: LPARAM,
    ) -> Result<usize, NativeTrayCaptureError> {
        let mut result = 0_usize;
        // SAFETY: Category 8 (FFI boundary). Messages are sent only to the
        // resolved Explorer ToolbarWindow32 with a fixed 250 ms abort-if-hung
        // timeout. LPARAM is either zero or this session's live remote buffer.
        let completed = unsafe {
            SendMessageTimeoutW(
                self.toolbar,
                message,
                wparam,
                lparam,
                SMTO_ABORTIFHUNG,
                TOOLBAR_MESSAGE_TIMEOUT_MS,
                Some(&mut result),
            )
        };
        if completed.0 == 0 {
            Err(NativeTrayCaptureError::ToolbarUnavailable)
        } else {
            Ok(result)
        }
    }
}

impl ProcessMemoryReader for Win32ToolbarSession {
    fn is_readable(&self, address: usize, byte_len: usize) -> bool {
        is_readable_remote_span(self.process.handle(), address, byte_len)
    }

    fn read_exact(&self, address: usize, destination: &mut [u8]) -> bool {
        let mut bytes_read = 0_usize;
        // SAFETY: Category 8 (FFI boundary). The remote span was validated
        // before candidate reads, and `destination` is fixed local writable
        // storage for the exact requested byte count.
        unsafe {
            ReadProcessMemory(
                self.process.handle(),
                address as *const c_void,
                destination.as_mut_ptr().cast(),
                destination.len(),
                Some(&mut bytes_read),
            )
        }
        .is_ok()
            && bytes_read == destination.len()
    }
}

impl ToolbarSession for Win32ToolbarSession {
    fn button_count(&mut self) -> Result<usize, NativeTrayCaptureError> {
        let count = self.toolbar_message(TB_BUTTONCOUNT, WPARAM(0), LPARAM(0))?;
        if count > i32::MAX as usize {
            Err(NativeTrayCaptureError::ToolbarUnavailable)
        } else {
            Ok(count)
        }
    }

    fn read_button(
        &mut self,
        index: usize,
        destination: &mut [u8; X64_TBBUTTON_BYTES],
    ) -> Result<(), NativeTrayCaptureError> {
        let copied = self.toolbar_message(
            TB_GETBUTTON,
            WPARAM(index),
            LPARAM(self.remote_button.address() as isize),
        )?;
        if copied == 0 {
            return Err(NativeTrayCaptureError::ToolbarUnavailable);
        }
        if self.read_exact(self.remote_button.address(), destination) {
            Ok(())
        } else {
            Err(NativeTrayCaptureError::ProcessMemoryUnavailable)
        }
    }
}

fn is_readable_remote_span(process: HANDLE, address: usize, byte_len: usize) -> bool {
    let Some(end) = address.checked_add(byte_len) else {
        return false;
    };
    if address == 0 || byte_len == 0 || end <= address {
        return false;
    }

    let mut cursor = address;
    while cursor < end {
        let mut information = MEMORY_BASIC_INFORMATION::default();
        // SAFETY: Category 8 (FFI boundary). `information` is initialized local
        // writable storage and the query only describes the copied remote
        // address; it does not read the candidate payload.
        let queried = unsafe {
            VirtualQueryEx(
                process,
                Some(cursor as *const c_void),
                &mut information,
                size_of::<MEMORY_BASIC_INFORMATION>(),
            )
        };
        if queried != size_of::<MEMORY_BASIC_INFORMATION>()
            || information.State != MEM_COMMIT
            || !is_readable_protection(information.Protect)
        {
            return false;
        }
        let region_start = information.BaseAddress as usize;
        let Some(region_end) = region_start.checked_add(information.RegionSize) else {
            return false;
        };
        if cursor < region_start || region_end <= cursor {
            return false;
        }
        cursor = region_end.min(end);
    }
    true
}

const fn is_readable_protection(protection: PAGE_PROTECTION_FLAGS) -> bool {
    if protection.contains(PAGE_GUARD) || protection.contains(PAGE_NOACCESS) {
        return false;
    }
    let base = protection.0 & 0xff;
    base == PAGE_READONLY.0
        || base == PAGE_READWRITE.0
        || base == PAGE_WRITECOPY.0
        || base == PAGE_EXECUTE_READ.0
        || base == PAGE_EXECUTE_READWRITE.0
        || base == PAGE_EXECUTE_WRITECOPY.0
}

struct Win32TrayOwnerResolver;

impl TrayOwnerResolver for Win32TrayOwnerResolver {
    fn resolve_owner(&self, owner_window: usize) -> Option<ResolvedTrayOwner> {
        let window = HWND(owner_window as *mut c_void);
        // SAFETY: Category 8 (FFI boundary). The decoded HWND is treated only as
        // a copied identity and must still identify a live window.
        if !unsafe { IsWindow(Some(window)) }.as_bool() {
            return None;
        }
        let process_id = window_process_id(window)?;
        validate_owner_process_id(process_id).ok()?;
        let process = OwnedProcess::open_owner(process_id)?;
        // SAFETY: Category 8 (FFI boundary). This parameterless handle query
        // confirms the opened process still matches the resolved owner PID.
        if unsafe { GetProcessId(process.handle()) } != process_id
            || window_process_id(window) != Some(process_id)
        {
            return None;
        }
        let executable = query_process_image_path(process.handle())?;
        Some(ResolvedTrayOwner {
            process_id,
            executable,
        })
    }
}

fn query_process_image_path(process: HANDLE) -> Option<String> {
    let mut buffer = vec![0_u16; 32_768];
    let mut length = buffer.len() as u32;
    // SAFETY: Category 8 (FFI boundary). The writable path buffer remains live
    // for the synchronous query and the owner handle has limited query rights.
    unsafe {
        QueryFullProcessImageNameW(
            process,
            PROCESS_NAME_WIN32,
            PWSTR(buffer.as_mut_ptr()),
            &mut length,
        )
    }
    .ok()?;
    (length > 0 && length as usize <= buffer.len())
        .then(|| String::from_utf16_lossy(&buffer[..length as usize]))
}

#[cfg(test)]
mod tests {
    use std::cell::RefCell;
    use std::collections::VecDeque;

    use crate::NativeWindowId;
    use crate::background_apps::{BackgroundAppEntry, BackgroundAppOrigin};
    use crate::native_tray::NativeTrayIdentity;
    use crate::tray_record_decoder::{MAX_NATIVE_TRAY_CANDIDATES, MAX_TRAY_TOOLTIP_CHARS};

    use super::{
        BOUNDED_TOOLTIP_BYTES, ExplorerTraySource, NativeTrayCaptureError,
        NativeTrayCaptureOutcome, ProcessMemoryReader, ResolvedTrayOwner, ToolbarSession, TrayHost,
        TrayHostCapture, TrayOwnerResolver, TrayToolbarReader, X64_TRAY_DATA_BYTES,
        decode_toolbar_entry, read_toolbar_session, validate_toolbar_process_id,
    };

    struct InjectedReader {
        results: VecDeque<TrayHostCapture>,
        requests: Vec<(TrayHost, u64, usize)>,
    }

    impl InjectedReader {
        fn new(results: impl IntoIterator<Item = TrayHostCapture>) -> Self {
            Self {
                results: results.into_iter().collect(),
                requests: Vec::new(),
            }
        }
    }

    impl TrayToolbarReader for InjectedReader {
        fn read_host(
            &mut self,
            host: TrayHost,
            generation: u64,
            remaining: usize,
        ) -> TrayHostCapture {
            self.requests.push((host, generation, remaining));
            self.results
                .pop_front()
                .expect("each requested host must have an injected result")
        }
    }

    fn native_entry(window: isize, icon_id: u32, generation: u64) -> BackgroundAppEntry {
        let identity = NativeTrayIdentity::new(
            NativeWindowId::new(window),
            42,
            icon_id,
            0x8001,
            0,
            None,
            generation,
        )
        .expect("synthetic native identity");
        BackgroundAppEntry::native(
            identity,
            format!("App {icon_id}"),
            format!(r"C:\Apps\app{icon_id}.exe"),
            format!(r"C:\Apps\app{icon_id}.exe"),
        )
    }

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    enum MemoryCall {
        Validate(usize, usize),
        Read(usize, usize),
    }

    struct InjectedMemory {
        tray_data_address: usize,
        tray_data: [u8; X64_TRAY_DATA_BYTES],
        tooltip_address: usize,
        tooltip: [u8; BOUNDED_TOOLTIP_BYTES],
        tooltip_readable: bool,
        calls: RefCell<Vec<MemoryCall>>,
    }

    impl ProcessMemoryReader for InjectedMemory {
        fn is_readable(&self, address: usize, byte_len: usize) -> bool {
            self.calls
                .borrow_mut()
                .push(MemoryCall::Validate(address, byte_len));
            (address == self.tray_data_address && byte_len == self.tray_data.len())
                || (self.tooltip_readable
                    && address == self.tooltip_address
                    && byte_len == self.tooltip.len())
        }

        fn read_exact(&self, address: usize, destination: &mut [u8]) -> bool {
            self.calls
                .borrow_mut()
                .push(MemoryCall::Read(address, destination.len()));
            let source = if address == self.tray_data_address {
                self.tray_data.as_slice()
            } else if address == self.tooltip_address {
                self.tooltip.as_slice()
            } else {
                return false;
            };
            if source.len() != destination.len() {
                return false;
            }
            destination.copy_from_slice(source);
            true
        }
    }

    struct InjectedOwners {
        owner_window: usize,
        owner: Option<ResolvedTrayOwner>,
    }

    impl TrayOwnerResolver for InjectedOwners {
        fn resolve_owner(&self, owner_window: usize) -> Option<ResolvedTrayOwner> {
            (owner_window == self.owner_window).then(|| {
                self.owner.as_ref().map(|owner| ResolvedTrayOwner {
                    process_id: owner.process_id,
                    executable: owner.executable.clone(),
                })
            })?
        }
    }

    fn x64_button(tray_data_address: usize, tooltip_address: usize) -> [u8; 32] {
        let mut button = [0_u8; 32];
        button[16..24].copy_from_slice(&(tray_data_address as u64).to_le_bytes());
        button[24..32].copy_from_slice(&(tooltip_address as u64).to_le_bytes());
        button
    }

    fn x64_tray_data(
        owner_window: usize,
        icon_id: u32,
        callback_message: u32,
        icon_handle: usize,
    ) -> [u8; X64_TRAY_DATA_BYTES] {
        let mut data = [0_u8; X64_TRAY_DATA_BYTES];
        data[0..8].copy_from_slice(&(owner_window as u64).to_le_bytes());
        data[8..12].copy_from_slice(&icon_id.to_le_bytes());
        data[12..16].copy_from_slice(&callback_message.to_le_bytes());
        data[24..32].copy_from_slice(&(icon_handle as u64).to_le_bytes());
        data
    }

    fn utf16_tooltip(value: &str) -> [u8; BOUNDED_TOOLTIP_BYTES] {
        let mut bytes = [0_u8; BOUNDED_TOOLTIP_BYTES];
        for (index, unit) in value
            .encode_utf16()
            .take(MAX_TRAY_TOOLTIP_CHARS)
            .enumerate()
        {
            let offset = index * size_of::<u16>();
            bytes[offset..offset + 2].copy_from_slice(&unit.to_le_bytes());
        }
        bytes
    }

    fn injected_candidate(tooltip_readable: bool) -> (InjectedMemory, InjectedOwners, [u8; 32]) {
        const TRAY_DATA_ADDRESS: usize = 0x2000;
        const TOOLTIP_ADDRESS: usize = 0x3000;
        const OWNER_WINDOW: usize = 0x4000;
        (
            InjectedMemory {
                tray_data_address: TRAY_DATA_ADDRESS,
                tray_data: x64_tray_data(OWNER_WINDOW, 9, 0x8001, 0x5000),
                tooltip_address: TOOLTIP_ADDRESS,
                tooltip: utf16_tooltip("Zoom - signed in"),
                tooltip_readable,
                calls: RefCell::new(Vec::new()),
            },
            InjectedOwners {
                owner_window: OWNER_WINDOW,
                owner: Some(ResolvedTrayOwner {
                    process_id: 42,
                    executable: String::from(r"C:\Apps\Zoom.exe"),
                }),
            },
            x64_button(TRAY_DATA_ADDRESS, TOOLTIP_ADDRESS),
        )
    }

    struct InjectedToolbarSession {
        memory: InjectedMemory,
        button: [u8; 32],
        count: usize,
        fail_at: Option<usize>,
        requested: Vec<usize>,
    }

    impl ProcessMemoryReader for InjectedToolbarSession {
        fn is_readable(&self, address: usize, byte_len: usize) -> bool {
            self.memory.is_readable(address, byte_len)
        }

        fn read_exact(&self, address: usize, destination: &mut [u8]) -> bool {
            self.memory.read_exact(address, destination)
        }
    }

    impl ToolbarSession for InjectedToolbarSession {
        fn button_count(&mut self) -> Result<usize, NativeTrayCaptureError> {
            Ok(self.count)
        }

        fn read_button(
            &mut self,
            index: usize,
            destination: &mut [u8; 32],
        ) -> Result<(), NativeTrayCaptureError> {
            self.requested.push(index);
            if self.fail_at == Some(index) {
                return Err(NativeTrayCaptureError::ToolbarUnavailable);
            }
            destination.copy_from_slice(&self.button);
            Ok(())
        }
    }

    fn injected_session(
        count: usize,
        fail_at: Option<usize>,
    ) -> (InjectedToolbarSession, InjectedOwners) {
        let (memory, owners, button) = injected_candidate(true);
        (
            InjectedToolbarSession {
                memory,
                button,
                count,
                fail_at,
                requested: Vec::new(),
            },
            owners,
        )
    }

    #[test]
    fn source_queries_notification_and_overflow_hosts_with_one_global_raw_limit() {
        let notification = vec![native_entry(7, 1, 31)];
        let overflow = vec![native_entry(8, 2, 31)];
        let reader = InjectedReader::new([
            TrayHostCapture::complete(notification, 1),
            TrayHostCapture::complete(overflow, 1),
        ]);
        let mut source = ExplorerTraySource::new(reader);

        let capture = source.capture(31);

        assert_eq!(
            source.reader.requests,
            vec![
                (TrayHost::Notification, 31, MAX_NATIVE_TRAY_CANDIDATES),
                (TrayHost::Overflow, 31, MAX_NATIVE_TRAY_CANDIDATES - 1),
            ]
        );
        assert_eq!(capture.entries.len(), 2);
        assert_eq!(capture.outcome, NativeTrayCaptureOutcome::Complete);
    }

    #[test]
    fn source_preserves_the_other_host_when_either_host_fails() {
        for (results, expected_label) in [
            (
                vec![
                    TrayHostCapture::failed(Vec::new(), 0, NativeTrayCaptureError::AccessDenied),
                    TrayHostCapture::complete(vec![native_entry(8, 2, 32)], 1),
                ],
                "App 2",
            ),
            (
                vec![
                    TrayHostCapture::complete(vec![native_entry(7, 1, 32)], 1),
                    TrayHostCapture::failed(
                        Vec::new(),
                        0,
                        NativeTrayCaptureError::ToolbarUnavailable,
                    ),
                ],
                "App 1",
            ),
        ] {
            let reader = InjectedReader::new(results);
            let mut source = ExplorerTraySource::new(reader);

            let capture = source.capture(32);

            assert_eq!(source.reader.requests.len(), 2);
            assert_eq!(capture.entries.len(), 1);
            assert_eq!(capture.entries[0].label(), expected_label);
            assert!(matches!(
                capture.outcome,
                NativeTrayCaptureOutcome::Partial(_)
            ));
        }
    }

    #[test]
    fn source_charges_malformed_raw_candidates_against_the_global_budget() {
        let reader = InjectedReader::new([
            TrayHostCapture::complete(Vec::new(), MAX_NATIVE_TRAY_CANDIDATES),
            TrayHostCapture::complete(Vec::new(), 0),
        ]);
        let mut source = ExplorerTraySource::new(reader);

        let capture = source.capture(33);

        assert_eq!(
            source.reader.requests,
            vec![
                (TrayHost::Notification, 33, MAX_NATIVE_TRAY_CANDIDATES),
                (TrayHost::Overflow, 33, 0),
            ]
        );
        assert!(capture.entries.is_empty());
        assert_eq!(capture.outcome, NativeTrayCaptureOutcome::Complete);
    }

    #[test]
    fn native_capture_errors_are_explicit_and_redacted() {
        for error in [
            NativeTrayCaptureError::Unsupported,
            NativeTrayCaptureError::AccessDenied,
            NativeTrayCaptureError::HostUnavailable,
            NativeTrayCaptureError::ToolbarUnavailable,
            NativeTrayCaptureError::ProcessMemoryUnavailable,
        ] {
            assert!(error.code().starts_with("native_tray_"));
            let diagnostic = format!("{error:?} {error}");
            assert!(!diagnostic.contains(r"C:\"));
            assert!(!diagnostic.contains("0x"));
        }
    }

    #[test]
    fn decoder_validates_full_remote_spans_before_reading_and_scopes_identity_to_generation() {
        let (memory, owners, button) = injected_candidate(true);

        let entry =
            decode_toolbar_entry(&memory, &owners, &button, 77).expect("valid injected tray entry");

        assert_eq!(entry.label(), "Zoom - signed in");
        assert_eq!(entry.executable(), r"C:\Apps\Zoom.exe");
        let BackgroundAppOrigin::Native(identity) = entry.origin() else {
            panic!("entry must retain native origin");
        };
        assert_eq!(identity.generation(), 77);
        assert_eq!(identity.version(), 0);
        assert_eq!(identity.guid(), None);
        assert_eq!(
            memory.calls.into_inner(),
            vec![
                MemoryCall::Validate(0x2000, X64_TRAY_DATA_BYTES),
                MemoryCall::Validate(0x3000, BOUNDED_TOOLTIP_BYTES),
                MemoryCall::Read(0x2000, X64_TRAY_DATA_BYTES),
                MemoryCall::Read(0x3000, BOUNDED_TOOLTIP_BYTES),
            ]
        );
    }

    #[test]
    fn decoder_skips_unreadable_tooltip_without_partially_reading_remote_data() {
        let (memory, owners, button) = injected_candidate(false);

        assert!(decode_toolbar_entry(&memory, &owners, &button, 78).is_none());
        assert_eq!(
            memory.calls.into_inner(),
            vec![
                MemoryCall::Validate(0x2000, X64_TRAY_DATA_BYTES),
                MemoryCall::Validate(0x3000, BOUNDED_TOOLTIP_BYTES),
            ]
        );
    }

    #[test]
    fn toolbar_session_caps_raw_reads_to_the_remaining_global_budget() {
        let (mut session, owners) = injected_session(MAX_NATIVE_TRAY_CANDIDATES + 44, None);

        let capture = read_toolbar_session(&mut session, &owners, 79, 1);

        assert_eq!(session.requested, vec![0]);
        assert_eq!(capture.entries.len(), 1);
        assert_eq!(capture.raw_candidates, 1);
        assert_eq!(capture.error, None);
    }

    #[test]
    fn toolbar_session_stops_the_host_after_the_first_failed_message() {
        let (mut session, owners) = injected_session(8, Some(1));

        let capture = read_toolbar_session(&mut session, &owners, 80, 8);

        assert_eq!(
            capture.error,
            Some(NativeTrayCaptureError::ToolbarUnavailable)
        );
        assert_eq!(capture.raw_candidates, 2);
        assert_eq!(session.requested, vec![0, 1]);
    }

    #[test]
    fn matching_toolbar_pid_is_accepted_and_mismatched_overflow_pid_is_rejected() {
        assert_eq!(validate_toolbar_process_id(42, 42), Ok(42));
        assert_eq!(
            validate_toolbar_process_id(42, 99),
            Err(NativeTrayCaptureError::HostUnavailable)
        );
        assert_eq!(
            validate_toolbar_process_id(0, 42),
            Err(NativeTrayCaptureError::HostUnavailable)
        );
        assert_eq!(
            validate_toolbar_process_id(42, 0),
            Err(NativeTrayCaptureError::HostUnavailable)
        );
    }
}
