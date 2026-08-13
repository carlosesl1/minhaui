use std::io;
use std::path::Path;
use std::time::Duration;

const RESTORE_QUIESCE_TIMEOUT: Duration = Duration::from_secs(20);

/// Cross-process ownership of one user's watchdog/child lifetime.
///
/// The owner is held from before the first recovery check until supervision
/// has terminated the child, closed its control pipe and completed final
/// recovery. A packaging restore takes the same gate before touching a journal.
pub struct SupervisionLifecycle {
    #[cfg(windows)]
    inner: crate::lifecycle_win32::SupervisionLifecycle,
}

impl SupervisionLifecycle {
    pub fn acquire(recovery_journal: &Path) -> io::Result<Self> {
        #[cfg(windows)]
        {
            crate::lifecycle_win32::SupervisionLifecycle::acquire(recovery_journal)
                .map(|inner| Self { inner })
        }
        #[cfg(not(windows))]
        {
            let _ = recovery_journal;
            Ok(Self {})
        }
    }

    #[must_use]
    pub fn shutdown_requested(&self) -> bool {
        #[cfg(windows)]
        {
            self.inner.shutdown_requested()
        }
        #[cfg(not(windows))]
        {
            false
        }
    }
}

/// Exclusive packaging gate returned only after an existing supervisor has
/// acknowledged shutdown by releasing ownership.
pub struct RestoreLifecycleGate {
    #[cfg(windows)]
    _inner: crate::lifecycle_win32::RestoreLifecycleGate,
}

pub fn quiesce_for_restore(recovery_journal: &Path) -> io::Result<RestoreLifecycleGate> {
    #[cfg(windows)]
    {
        crate::ensure_current_user_session_exclusive()?;
        crate::lifecycle_win32::quiesce_for_restore(recovery_journal, RESTORE_QUIESCE_TIMEOUT)
            .map(|inner| RestoreLifecycleGate { _inner: inner })
    }
    #[cfg(not(windows))]
    {
        let _ = (recovery_journal, RESTORE_QUIESCE_TIMEOUT);
        Ok(RestoreLifecycleGate {})
    }
}
