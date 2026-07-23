use std::cell::Cell;

use crate::TopbarSnapshot;
use crate::win32_shell_observation::{
    ShellObservation, ShellObservationRuntime, ShellObservationSource,
};

struct CountingSource {
    captures: Cell<u32>,
}

impl ShellObservationSource for CountingSource {
    fn capture(&mut self, _now_ms: u64) -> windows::core::Result<ShellObservation> {
        self.captures.set(self.captures.get() + 1);
        Ok(ShellObservation::new(
            Vec::new(),
            TopbarSnapshot::privacy_safe_fixture(),
        ))
    }
}

#[test]
fn process_observation_deduplicates_ticks_and_unchanged_snapshots()
-> Result<(), Box<dyn std::error::Error>> {
    let source = CountingSource {
        captures: Cell::new(0),
    };
    let mut runtime = ShellObservationRuntime::with_source(source, 1_000);

    assert!(runtime.refresh_if_changed(1_000)?);
    assert!(!runtime.refresh_if_changed(1_010)?);
    assert!(!runtime.refresh_if_changed(1_999)?);
    assert!(!runtime.refresh_if_changed(2_000)?);
    assert_eq!(runtime.source().captures.get(), 2);
    Ok(())
}

#[test]
fn identical_observation_does_not_report_a_visual_change() -> Result<(), Box<dyn std::error::Error>>
{
    let source = CountingSource {
        captures: Cell::new(0),
    };
    let mut runtime = ShellObservationRuntime::with_source(source, 1_000);

    assert!(runtime.refresh_if_changed(1_000)?);
    assert!(!runtime.refresh_if_changed(2_000)?);
    assert_eq!(runtime.source().captures.get(), 2);
    Ok(())
}
