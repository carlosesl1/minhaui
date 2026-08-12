use std::sync::Arc;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DockItemVisualKind {
    App,
    Separator,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RunningIndicator {
    Stopped,
    Running,
    Focused,
    Minimized,
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum DockIcon {
    WindowsExecutable(Arc<str>),
    #[default]
    SystemFallback,
}

impl DockIcon {
    #[must_use]
    pub fn windows_executable(source: &str) -> Self {
        Self::WindowsExecutable(source.into())
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct DockItemVisual {
    id: u64,
    label: Arc<str>,
    kind: DockItemVisualKind,
    indicator: RunningIndicator,
    icon: DockIcon,
}

impl DockItemVisual {
    #[must_use]
    pub fn app(id: u64, label: &str, indicator: RunningIndicator) -> Self {
        Self {
            id,
            label: label.into(),
            kind: DockItemVisualKind::App,
            indicator,
            icon: DockIcon::SystemFallback,
        }
    }

    #[must_use]
    pub fn app_with_icon(
        id: u64,
        label: &str,
        indicator: RunningIndicator,
        icon: DockIcon,
    ) -> Self {
        Self {
            id,
            label: label.into(),
            kind: DockItemVisualKind::App,
            indicator,
            icon,
        }
    }

    #[must_use]
    pub fn separator(id: u64) -> Self {
        Self {
            id,
            label: Arc::default(),
            kind: DockItemVisualKind::Separator,
            indicator: RunningIndicator::Stopped,
            icon: DockIcon::SystemFallback,
        }
    }

    #[must_use]
    pub const fn id(&self) -> u64 {
        self.id
    }

    #[must_use]
    pub fn label(&self) -> &str {
        &self.label
    }

    #[must_use]
    pub const fn kind(&self) -> DockItemVisualKind {
        self.kind
    }

    #[must_use]
    pub const fn indicator(&self) -> RunningIndicator {
        self.indicator
    }

    #[must_use]
    pub const fn icon(&self) -> &DockIcon {
        &self.icon
    }
}
