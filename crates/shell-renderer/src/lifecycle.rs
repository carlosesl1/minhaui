#![deny(unsafe_code)]

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceEvent {
    DeviceRemoved,
    DeviceReset,
    ResourcesRebuilt,
    ShutdownRequested,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DeviceLifecycle {
    Ready,
    Rebuilding,
    ShuttingDown,
}

impl DeviceLifecycle {
    #[must_use]
    pub const fn ready() -> Self {
        Self::Ready
    }

    #[must_use]
    pub const fn transition(self, event: DeviceEvent) -> Self {
        match (self, event) {
            (Self::Ready, DeviceEvent::DeviceRemoved | DeviceEvent::DeviceReset) => {
                Self::Rebuilding
            }
            (Self::Ready, DeviceEvent::ResourcesRebuilt) => Self::Ready,
            (Self::Ready, DeviceEvent::ShutdownRequested) => Self::ShuttingDown,
            (Self::Rebuilding, DeviceEvent::ResourcesRebuilt) => Self::Ready,
            (Self::Rebuilding, DeviceEvent::DeviceRemoved | DeviceEvent::DeviceReset) => {
                Self::Rebuilding
            }
            (Self::Rebuilding, DeviceEvent::ShutdownRequested) => Self::ShuttingDown,
            (Self::ShuttingDown, _) => Self::ShuttingDown,
        }
    }
}
