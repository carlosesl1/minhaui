use crate::native_event_route::NativeWindowId;
use std::hash::{Hash, Hasher};

/// The immutable identity supplied by a native notification-area icon.
///
/// This is deliberately a value type rather than an HWND wrapper.  The
/// platform adapter can construct it from its observed values and later
/// consumers can compare it without crossing the FFI boundary.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct NativeTrayIdentity {
    owner_window: NativeWindowId,
    owner_process_id: u32,
    icon_id: u32,
    callback_message: u32,
    version: u32,
    guid: Option<[u8; 16]>,
    generation: u64,
}

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(crate) enum NativeTraySelector {
    Guid([u8; 16]),
    IconId(u32),
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct NativeTrayKey {
    owner_window: NativeWindowId,
    selector: NativeTraySelector,
}

impl Hash for NativeTrayKey {
    fn hash<H: Hasher>(&self, state: &mut H) {
        self.owner_window.value().hash(state);
        self.selector.hash(state);
    }
}

impl NativeTrayIdentity {
    /// Builds an identity when the values required to route a tray callback
    /// are present.  Zero owner windows, process IDs, and callback messages
    /// cannot identify a live tray icon and are rejected.
    #[allow(
        dead_code,
        reason = "native tray identity construction is consumed by the adapter incrementally"
    )]
    #[must_use]
    pub(crate) const fn new(
        owner_window: NativeWindowId,
        owner_process_id: u32,
        icon_id: u32,
        callback_message: u32,
        version: u32,
        guid: Option<[u8; 16]>,
        generation: u64,
    ) -> Option<Self> {
        if owner_window.value() == 0 || owner_process_id == 0 || callback_message == 0 {
            return None;
        }

        Some(Self {
            owner_window,
            owner_process_id,
            icon_id,
            callback_message,
            version,
            guid,
            generation,
        })
    }

    #[allow(
        dead_code,
        reason = "native tray routing metadata is consumed by the adapter incrementally"
    )]
    #[must_use]
    pub(crate) const fn owner_window(self) -> NativeWindowId {
        self.owner_window
    }

    #[allow(
        dead_code,
        reason = "native tray routing metadata is consumed by the adapter incrementally"
    )]
    #[must_use]
    pub(crate) const fn owner_process_id(self) -> u32 {
        self.owner_process_id
    }

    #[allow(
        dead_code,
        reason = "native tray routing metadata is consumed by the adapter incrementally"
    )]
    #[must_use]
    pub(crate) const fn icon_id(self) -> u32 {
        self.icon_id
    }

    #[allow(
        dead_code,
        reason = "native tray routing metadata is consumed by the adapter incrementally"
    )]
    #[must_use]
    pub(crate) const fn callback_message(self) -> u32 {
        self.callback_message
    }

    #[allow(
        dead_code,
        reason = "native tray routing metadata is consumed by the adapter incrementally"
    )]
    #[must_use]
    pub(crate) const fn version(self) -> u32 {
        self.version
    }

    #[allow(
        dead_code,
        reason = "native tray routing metadata is consumed by the adapter incrementally"
    )]
    #[must_use]
    pub(crate) const fn guid(self) -> Option<[u8; 16]> {
        self.guid
    }

    #[must_use]
    pub(crate) const fn logical_key(self) -> NativeTrayKey {
        NativeTrayKey {
            owner_window: self.owner_window,
            selector: match self.guid {
                Some(guid) => NativeTraySelector::Guid(guid),
                None => NativeTraySelector::IconId(self.icon_id),
            },
        }
    }

    #[allow(
        dead_code,
        reason = "native tray generation is consumed by the adapter incrementally"
    )]
    #[must_use]
    pub(crate) const fn generation(self) -> u64 {
        self.generation
    }

    #[allow(
        dead_code,
        reason = "native tray generation validation is consumed by the adapter incrementally"
    )]
    #[must_use]
    pub(crate) const fn is_current(self, generation: u64) -> bool {
        self.generation == generation
    }
}

impl NativeTrayKey {
    #[must_use]
    pub(crate) const fn owner_window(self) -> NativeWindowId {
        self.owner_window
    }

    #[must_use]
    pub(crate) const fn selector(self) -> NativeTraySelector {
        self.selector
    }
}

#[cfg(test)]
mod tests {
    use super::NativeTrayIdentity;
    use crate::native_event_route::NativeWindowId;

    fn identity() -> NativeTrayIdentity {
        NativeTrayIdentity::new(NativeWindowId::new(7), 42, 11, 0x8001, 4, None, 9)
            .expect("valid native identity")
    }

    #[test]
    fn rejects_zero_owner_process_and_callback_identity_values() {
        assert!(NativeTrayIdentity::new(NativeWindowId::new(0), 42, 11, 1, 1, None, 1).is_none());
        assert!(NativeTrayIdentity::new(NativeWindowId::new(1), 0, 11, 1, 1, None, 1).is_none());
        assert!(NativeTrayIdentity::new(NativeWindowId::new(1), 42, 11, 0, 1, None, 1).is_none());
    }

    #[test]
    fn exposes_identity_values_and_rejects_stale_generations() {
        let identity = identity();
        assert_eq!(identity.owner_window().value(), 7);
        assert_eq!(identity.owner_process_id(), 42);
        assert_eq!(identity.icon_id(), 11);
        assert_eq!(identity.callback_message(), 0x8001);
        assert_eq!(identity.version(), 4);
        assert_eq!(identity.guid(), None);
        assert_eq!(identity.generation(), 9);
        assert!(identity.is_current(9));
        assert!(!identity.is_current(8));
    }
}
