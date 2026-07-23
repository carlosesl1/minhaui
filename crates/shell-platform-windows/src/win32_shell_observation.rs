#![deny(unsafe_code)]

use windows::core::Result;

use crate::win32_discovery::{WindowIdentityCache, discover_running_windows};
use crate::win32_preview_qa::seed_restricted_preview_for_qa;
use crate::win32_topbar_status::TopbarStatusReader;
use crate::{ObservedWindow, TopbarSnapshot, foreground_app_label};

#[derive(Clone, Debug, PartialEq)]
pub(super) struct ShellObservation {
    windows: Vec<ObservedWindow>,
    topbar: TopbarSnapshot,
}

impl ShellObservation {
    pub(super) const fn new(windows: Vec<ObservedWindow>, topbar: TopbarSnapshot) -> Self {
        Self { windows, topbar }
    }

    pub(super) fn windows(&self) -> &[ObservedWindow] {
        &self.windows
    }

    pub(super) const fn topbar(&self) -> &TopbarSnapshot {
        &self.topbar
    }
}

pub(super) trait ShellObservationSource {
    fn capture(&mut self, now_ms: u64) -> Result<ShellObservation>;
}

#[derive(Default)]
pub(super) struct WindowsShellObservationSource {
    identities: WindowIdentityCache,
    topbar: TopbarStatusReader,
}

impl ShellObservationSource for WindowsShellObservationSource {
    fn capture(&mut self, now_ms: u64) -> Result<ShellObservation> {
        let mut windows = discover_running_windows(&[], true, &mut self.identities)?;
        seed_restricted_preview_for_qa(&mut windows)?;
        let app_label = foreground_app_label(&windows);
        let topbar = self.topbar.snapshot(now_ms).with_app_label(&app_label);
        Ok(ShellObservation::new(windows, topbar))
    }
}

pub(super) struct ShellObservationRuntime<S = WindowsShellObservationSource> {
    source: S,
    minimum_interval_ms: u64,
    last_refresh_ms: Option<u64>,
    current: Option<ShellObservation>,
}

impl ShellObservationRuntime<WindowsShellObservationSource> {
    pub(super) fn new(minimum_interval_ms: u64) -> Self {
        Self::with_source(
            WindowsShellObservationSource::default(),
            minimum_interval_ms,
        )
    }
}

impl<S: ShellObservationSource> ShellObservationRuntime<S> {
    pub(super) const fn with_source(source: S, minimum_interval_ms: u64) -> Self {
        Self {
            source,
            minimum_interval_ms,
            last_refresh_ms: None,
            current: None,
        }
    }

    pub(super) fn refresh_if_changed(&mut self, now_ms: u64) -> Result<bool> {
        if self
            .last_refresh_ms
            .is_some_and(|last| now_ms.saturating_sub(last) < self.minimum_interval_ms)
        {
            return Ok(false);
        }
        let observation = self.source.capture(now_ms)?;
        self.last_refresh_ms = Some(now_ms);
        if self.current.as_ref() == Some(&observation) {
            return Ok(false);
        }
        self.current = Some(observation);
        Ok(true)
    }

    pub(super) const fn current(&self) -> Option<&ShellObservation> {
        self.current.as_ref()
    }

    #[cfg(test)]
    pub(super) const fn source(&self) -> &S {
        &self.source
    }
}
