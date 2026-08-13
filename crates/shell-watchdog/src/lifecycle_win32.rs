use std::io;
use std::path::Path;
use std::time::Duration;

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, GetLastError, HANDLE, WAIT_ABANDONED, WAIT_OBJECT_0,
    WAIT_TIMEOUT,
};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, ReleaseMutex, ResetEvent, SetEvent, WaitForSingleObject,
};
use windows::core::PCWSTR;

struct OwnedHandle(HANDLE);

impl Drop for OwnedHandle {
    fn drop(&mut self) {
        // SAFETY: The handle is created exactly once by this module and closed
        // exactly once when its owning guard leaves scope.
        let _ = unsafe { CloseHandle(self.0) };
    }
}

pub(super) struct SupervisionLifecycle {
    owner: OwnedHandle,
    shutdown: OwnedHandle,
}

impl SupervisionLifecycle {
    pub(super) fn acquire(recovery_journal: &Path) -> io::Result<Self> {
        let names = LifecycleNames::for_journal(recovery_journal)?;
        let owner = create_mutex(&names.owner, false)?;
        // SAFETY: GetLastError is read immediately after CreateMutexW and is
        // used only to distinguish a pre-existing named lifecycle object.
        let already_exists = unsafe { GetLastError() } == ERROR_ALREADY_EXISTS;
        if already_exists {
            return Err(io::Error::new(
                io::ErrorKind::AlreadyExists,
                "another watchdog or lifecycle recovery owns this session",
            ));
        }
        let shutdown = create_event(&names.shutdown)?;
        let lifecycle = Self { owner, shutdown };
        if lifecycle.shutdown_requested() {
            return Err(io::Error::new(
                io::ErrorKind::Interrupted,
                "a lifecycle recovery request is already active",
            ));
        }
        // SAFETY: The object was newly created by this process. A zero-time
        // wait acquires its initially-unowned mutex without racing another
        // opener, because any opener sees ERROR_ALREADY_EXISTS and yields.
        if unsafe { WaitForSingleObject(lifecycle.owner.0, 0) } != WAIT_OBJECT_0 {
            return Err(io::Error::new(
                io::ErrorKind::WouldBlock,
                "could not acquire the newly-created supervision lifecycle",
            ));
        }
        Ok(lifecycle)
    }

    pub(super) fn shutdown_requested(&self) -> bool {
        // SAFETY: Zero-time wait is a read-only poll of a live event handle.
        (unsafe { WaitForSingleObject(self.shutdown.0, 0) }) == WAIT_OBJECT_0
    }
}

impl Drop for SupervisionLifecycle {
    fn drop(&mut self) {
        // SAFETY: This process acquired the mutex with initial ownership and
        // releases it once, only after child shutdown and final restoration.
        let _ = unsafe { ReleaseMutex(self.owner.0) };
    }
}

pub(super) struct RestoreLifecycleGate {
    owner: OwnedHandle,
    _shutdown: OwnedHandle,
}

impl Drop for RestoreLifecycleGate {
    fn drop(&mut self) {
        // Clear the request only while still owning the lifecycle mutex. A new
        // supervisor cannot observe a cleared event before the packaging
        // mutation and final restore have both completed.
        // SAFETY: `_shutdown` is the live manual-reset event paired with this
        // owner and remains valid until this guard is dropped.
        let _ = unsafe { ResetEvent(self._shutdown.0) };
        // SAFETY: Successful quiescence transfers mutex ownership to this
        // guard; release occurs after the caller's final restore attempt.
        let _ = unsafe { ReleaseMutex(self.owner.0) };
    }
}

pub(super) fn quiesce_for_restore(
    recovery_journal: &Path,
    timeout: Duration,
) -> io::Result<RestoreLifecycleGate> {
    let names = LifecycleNames::for_journal(recovery_journal)?;
    // Reserve a reference to the owner object before signaling. This prevents
    // a new supervisor from creating/resetting state between the signal and
    // our wait, even when no supervisor was active at entry.
    let owner = create_mutex(&names.owner, false)?;
    let shutdown = create_event(&names.shutdown)?;
    // SAFETY: The named manual-reset event is live. Signaling is idempotent and
    // tells any owner to kill the child, close the pipe and stop restarting.
    unsafe { SetEvent(shutdown.0) }.map_err(win32_error)?;

    let timeout_ms = u32::try_from(timeout.as_millis()).unwrap_or(u32::MAX);
    // SAFETY: The mutex handle remains owned by this stack for the wait.
    match unsafe { WaitForSingleObject(owner.0, timeout_ms) } {
        WAIT_OBJECT_0 | WAIT_ABANDONED => Ok(RestoreLifecycleGate {
            owner,
            _shutdown: shutdown,
        }),
        WAIT_TIMEOUT => Err(io::Error::new(
            io::ErrorKind::TimedOut,
            "watchdog did not quiesce before the lifecycle recovery deadline",
        )),
        _ => Err(io::Error::last_os_error()),
    }
}

struct LifecycleNames {
    owner: Vec<u16>,
    shutdown: Vec<u16>,
}

impl LifecycleNames {
    fn for_journal(path: &Path) -> io::Result<Self> {
        if path.file_name().and_then(|value| value.to_str()) != Some("taskbar-recovery-v1.json") {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "lifecycle authority requires the canonical V1 journal path",
            ));
        }
        if !path.is_absolute() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "lifecycle authority requires an absolute journal path",
            ));
        }
        let normalized = path
            .as_os_str()
            .to_string_lossy()
            .replace('/', "\\")
            .to_ascii_lowercase();
        let mut first = 0xcbf2_9ce4_8422_2325_u64;
        let mut second = 0x8422_2325_cbf2_9ce4_u64;
        for byte in normalized.bytes() {
            first = (first ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
            second = (second ^ u64::from(byte)).wrapping_mul(0x0000_0100_0000_01b3);
        }
        let token = format!("{first:016x}{second:016x}");
        Ok(Self {
            // Hashing the absolute authority path keeps the production helper
            // and supervisor aligned while isolating explicit debug test roots.
            owner: wide_name(&format!("Local\\MinhaUi.Watchdog.Owner.v1.{token}")),
            shutdown: wide_name(&format!("Local\\MinhaUi.Watchdog.Shutdown.v1.{token}")),
        })
    }
}

fn wide_name(name: &str) -> Vec<u16> {
    name.encode_utf16().chain(std::iter::once(0)).collect()
}

fn create_mutex(name: &[u16], initial_owner: bool) -> io::Result<OwnedHandle> {
    // SAFETY: `name` is a NUL-terminated immutable UTF-16 buffer and no custom
    // security descriptor is supplied; Local namespace scopes it to a session.
    unsafe { CreateMutexW(None, initial_owner, PCWSTR(name.as_ptr())) }
        .map(OwnedHandle)
        .map_err(win32_error)
}

fn create_event(name: &[u16]) -> io::Result<OwnedHandle> {
    // SAFETY: Same bounded named-object contract as `create_mutex`; the event
    // is manual-reset so every supervisor poll observes shutdown until exit.
    unsafe { CreateEventW(None, true, false, PCWSTR(name.as_ptr())) }
        .map(OwnedHandle)
        .map_err(win32_error)
}

fn win32_error(error: windows::core::Error) -> io::Error {
    io::Error::other(error.to_string())
}

#[cfg(test)]
mod tests {
    use super::{LifecycleNames, SupervisionLifecycle, quiesce_for_restore};
    use std::path::Path;
    use std::sync::{Mutex, MutexGuard};
    use std::time::Duration;
    use windows::Win32::Foundation::{CloseHandle, HANDLE};

    static TEST_LOCK: Mutex<()> = Mutex::new(());

    fn serial() -> MutexGuard<'static, ()> {
        TEST_LOCK.lock().unwrap_or_else(|error| error.into_inner())
    }

    #[test]
    fn lifecycle_names_are_stable_and_do_not_expose_profile_paths() {
        let _serial = serial();
        let first =
            LifecycleNames::for_journal(Path::new(r"C:\Users\Alice\taskbar-recovery-v1.json"))
                .expect("names");
        let second =
            LifecycleNames::for_journal(Path::new(r"c:\users\alice\taskbar-recovery-v1.json"))
                .expect("names");
        assert_eq!(first.owner, second.owner);
        let owner = String::from_utf16_lossy(&first.owner);
        assert!(!owner.contains("Alice"));

        let isolated = LifecycleNames::for_journal(Path::new(
            r"C:\Users\Alice\test-state\taskbar-recovery-v1.json",
        ))
        .expect("isolated names");
        assert_ne!(first.owner, isolated.owner);
    }

    #[test]
    fn preexisting_shutdown_signal_prevents_a_new_supervisor() {
        let _serial = serial();
        let path = std::env::temp_dir()
            .join("minha-ui-lifecycle-preexisting-signal/taskbar-recovery-v1.json");
        let names = LifecycleNames::for_journal(&path).expect("names");
        let event = super::create_event(&names.shutdown).expect("event");
        // SAFETY: Test owns a live manual-reset event handle.
        unsafe { windows::Win32::System::Threading::SetEvent(event.0) }.expect("signal");

        let error = SupervisionLifecycle::acquire(&path)
            .err()
            .expect("active recovery signal must block startup");
        assert_eq!(error.kind(), std::io::ErrorKind::Interrupted);
    }

    #[test]
    fn restore_times_out_while_a_live_owner_has_not_quiesced() {
        let _serial = serial();
        let path = std::env::temp_dir().join(format!(
            "minha-ui-lifecycle-live-owner-{}/taskbar-recovery-v1.json",
            std::process::id()
        ));
        let owner_path = path.clone();
        let (ready_sender, ready_receiver) = std::sync::mpsc::channel();
        let (release_sender, release_receiver) = std::sync::mpsc::channel();
        let owner = std::thread::spawn(move || {
            let lifecycle = SupervisionLifecycle::acquire(&owner_path).expect("owner");
            ready_sender.send(()).expect("ready");
            release_receiver.recv().expect("release owner");
            assert!(lifecycle.shutdown_requested());
        });
        ready_receiver.recv().expect("owner ready");

        let error = quiesce_for_restore(&path, Duration::from_millis(10))
            .err()
            .expect("live owner must not be bypassed");
        assert_eq!(error.kind(), std::io::ErrorKind::TimedOut);
        release_sender.send(()).expect("release owner");
        owner.join().expect("owner thread");
    }

    #[test]
    fn restore_signal_waits_until_owner_acknowledges_shutdown() {
        let _serial = serial();
        let path = std::env::temp_dir().join(format!(
            "minha-ui-lifecycle-shutdown-race-{}/taskbar-recovery-v1.json",
            std::process::id()
        ));
        let owner_path = path.clone();
        let (ready_sender, ready_receiver) = std::sync::mpsc::channel();
        let owner = std::thread::spawn(move || {
            let lifecycle = SupervisionLifecycle::acquire(&owner_path).expect("owner");
            ready_sender.send(()).expect("ready");
            while !lifecycle.shutdown_requested() {
                std::thread::yield_now();
            }
            // Dropping here acknowledges that child and pipe cleanup would
            // already be complete in the real supervisor.
        });
        ready_receiver.recv().expect("owner ready");

        let _gate = quiesce_for_restore(&path, Duration::from_secs(1))
            .expect("restore acquires ownership only after acknowledgement");
        owner.join().expect("owner thread");
    }

    #[test]
    fn reserved_restore_object_prevents_startup_before_signal() {
        let _serial = serial();
        let path = std::env::temp_dir()
            .join("minha-ui-lifecycle-reserved-before-signal/taskbar-recovery-v1.json");
        let names = LifecycleNames::for_journal(&path).expect("names");
        let _reservation = super::create_mutex(&names.owner, false).expect("reserve owner name");

        let error = SupervisionLifecycle::acquire(&path)
            .err()
            .expect("reservation must close the pre-signal startup race");
        assert_eq!(error.kind(), std::io::ErrorKind::AlreadyExists);
    }

    #[test]
    fn abandoned_owner_is_recoverable_after_owner_exit() {
        let _serial = serial();
        let path = std::env::temp_dir().join(format!(
            "minha-ui-lifecycle-abandoned-owner-{}/taskbar-recovery-v1.json",
            std::process::id()
        ));
        let child_path = path.clone();
        let (handles_sender, handles_receiver) = std::sync::mpsc::channel();
        std::thread::spawn(move || {
            let owner = SupervisionLifecycle::acquire(&child_path).expect("owner");
            handles_sender
                .send((owner.owner.0.0 as isize, owner.shutdown.0.0 as isize))
                .expect("handles");
            std::mem::forget(owner);
        })
        .join()
        .expect("owner thread");

        let gate = quiesce_for_restore(&path, Duration::from_secs(1))
            .expect("abandoned mutex transfers ownership");
        drop(gate);
        let (owner, shutdown) = handles_receiver.recv().expect("leaked handles");
        // SAFETY: The owning test thread deliberately leaked these two handles
        // after transferring their raw values; cleanup closes each exactly once.
        let _ = unsafe { CloseHandle(HANDLE(owner as *mut _)) };
        // SAFETY: Same cleanup ownership as the paired mutex handle above.
        let _ = unsafe { CloseHandle(HANDLE(shutdown as *mut _)) };
    }
}
