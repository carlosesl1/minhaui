use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread::ThreadId;
use std::time::Duration;

use crate::TopbarSnapshot;
use crate::win32_shell_observation::{
    ShellObservation, ShellObservationRuntime, ShellObservationSource, ShellObservationUpdate,
};

struct CountingSource {
    captures: Arc<AtomicU32>,
}

impl ShellObservationSource for CountingSource {
    fn capture(&mut self, _now_ms: u64) -> windows::core::Result<ShellObservation> {
        self.captures.fetch_add(1, Ordering::AcqRel);
        Ok(ShellObservation::new(
            Vec::new(),
            TopbarSnapshot::privacy_safe_fixture(),
        ))
    }
}

#[test]
fn process_observation_deduplicates_ticks_and_unchanged_snapshots()
-> Result<(), Box<dyn std::error::Error>> {
    let captures = Arc::new(AtomicU32::new(0));
    let source = CountingSource {
        captures: Arc::clone(&captures),
    };
    let (result_tx, result_rx) = mpsc::channel();
    let mut runtime = ShellObservationRuntime::with_source(source, 1_000, 1_000, move |result| {
        let _ = result_tx.send(result);
    })?;
    assert!(!runtime.request_refresh(1_010));
    assert!(!runtime.request_refresh(1_999));
    assert!(runtime.request_refresh(2_000));
    assert!(!runtime.request_refresh(2_000));
    let result = result_rx.recv_timeout(Duration::from_secs(1))?;
    assert_eq!(runtime.complete(result), ShellObservationUpdate::Unchanged);
    assert_eq!(captures.load(Ordering::Acquire), 2);
    Ok(())
}

struct ThreadRecordingSource {
    captures: mpsc::Sender<ThreadId>,
}

impl ShellObservationSource for ThreadRecordingSource {
    fn capture(&mut self, _now_ms: u64) -> windows::core::Result<ShellObservation> {
        let _ = self.captures.send(std::thread::current().id());
        Ok(ShellObservation::new(
            Vec::new(),
            TopbarSnapshot::privacy_safe_fixture(),
        ))
    }
}

#[test]
fn scheduled_refresh_captures_on_the_owned_worker_instead_of_the_ui_thread()
-> Result<(), Box<dyn std::error::Error>> {
    let ui_thread = std::thread::current().id();
    let (capture_tx, capture_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    let mut runtime = ShellObservationRuntime::with_source(
        ThreadRecordingSource {
            captures: capture_tx,
        },
        1_000,
        1_000,
        move |result| {
            let _ = result_tx.send(result);
        },
    )?;
    assert_eq!(
        capture_rx.recv_timeout(Duration::from_secs(1))?,
        ui_thread,
        "initial startup capture remains on the caller before the message loop"
    );

    assert!(runtime.request_refresh(2_000));
    let worker_thread = capture_rx.recv_timeout(Duration::from_secs(1))?;
    let result = result_rx.recv_timeout(Duration::from_secs(1))?;

    assert_ne!(worker_thread, ui_thread);
    assert_eq!(runtime.complete(result), ShellObservationUpdate::Unchanged);
    Ok(())
}

struct SlowObservationSource {
    captures: Arc<AtomicU32>,
    slow_capture_started: mpsc::Sender<()>,
    release_slow_capture: Arc<(Mutex<bool>, Condvar)>,
}

impl ShellObservationSource for SlowObservationSource {
    fn capture(&mut self, _now_ms: u64) -> windows::core::Result<ShellObservation> {
        let capture = self.captures.fetch_add(1, Ordering::AcqRel);
        if capture == 1 {
            let _ = self.slow_capture_started.send(());
            let (released, wake) = &*self.release_slow_capture;
            let mut released = released
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            while !*released {
                released = wake
                    .wait(released)
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
            }
        }
        Ok(ShellObservation::new(
            Vec::new(),
            TopbarSnapshot::privacy_safe_fixture(),
        ))
    }
}

#[test]
fn result_is_stale_when_a_newer_refresh_was_requested_while_capture_was_running()
-> Result<(), Box<dyn std::error::Error>> {
    let captures = Arc::new(AtomicU32::new(0));
    let release_slow_capture = Arc::new((Mutex::new(false), Condvar::new()));
    let (started_tx, started_rx) = mpsc::channel();
    let (result_tx, result_rx) = mpsc::channel();
    let mut runtime = ShellObservationRuntime::with_source(
        SlowObservationSource {
            captures: Arc::clone(&captures),
            slow_capture_started: started_tx,
            release_slow_capture: Arc::clone(&release_slow_capture),
        },
        1_000,
        1_000,
        move |result| {
            let _ = result_tx.send(result);
        },
    )?;
    assert!(runtime.request_refresh(2_000));
    started_rx.recv_timeout(Duration::from_secs(1))?;
    assert!(runtime.request_refresh(3_000));
    {
        let (released, wake) = &*release_slow_capture;
        *released
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = true;
        wake.notify_all();
    }

    let stale = result_rx.recv_timeout(Duration::from_secs(1))?;
    assert_eq!(runtime.complete(stale), ShellObservationUpdate::Stale);
    let latest = result_rx.recv_timeout(Duration::from_secs(1))?;
    assert_eq!(runtime.complete(latest), ShellObservationUpdate::Unchanged);
    assert_eq!(captures.load(Ordering::Acquire), 3);
    Ok(())
}
