#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct TrayBridgeRecord {
    owner_window: isize,
    icon_id: u32,
    callback_message: u32,
    version: u32,
    guid: Option<[u8; 16]>,
    tooltip: String,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct TrayBridgeIdentity {
    pub(crate) owner_window: isize,
    pub(crate) icon_id: u32,
    pub(crate) guid: Option<[u8; 16]>,
}

impl TrayBridgeRecord {
    #[must_use]
    pub(crate) fn new(
        identity: TrayBridgeIdentity,
        callback_message: u32,
        version: u32,
        tooltip: impl Into<String>,
    ) -> Self {
        Self {
            owner_window: identity.owner_window,
            icon_id: identity.icon_id,
            callback_message,
            version,
            guid: identity.guid,
            tooltip: tooltip.into(),
        }
    }

    #[must_use]
    pub(crate) const fn owner_window(&self) -> isize {
        self.owner_window
    }

    #[must_use]
    pub(crate) const fn icon_id(&self) -> u32 {
        self.icon_id
    }

    #[must_use]
    pub(crate) const fn callback_message(&self) -> u32 {
        self.callback_message
    }

    #[must_use]
    pub(crate) const fn version(&self) -> u32 {
        self.version
    }

    #[must_use]
    pub(crate) const fn guid(&self) -> Option<[u8; 16]> {
        self.guid
    }

    #[must_use]
    pub(crate) fn tooltip(&self) -> &str {
        &self.tooltip
    }

    fn same_identity(&self, other: &Self) -> bool {
        self.owner_window == other.owner_window
            && match (self.guid, other.guid) {
                (Some(left), Some(right)) => left == right,
                (None, None) => self.icon_id == other.icon_id,
                _ => false,
            }
    }

    fn merge_from(&mut self, update: Self) {
        if update.callback_message != 0 {
            self.callback_message = update.callback_message;
        }
        if update.version != 0 {
            self.version = update.version;
        }
        if !update.tooltip.is_empty() {
            self.tooltip = update.tooltip;
        }
    }
}

#[derive(Default)]
pub(crate) struct TrayBridgeCatalog {
    records: Vec<TrayBridgeRecord>,
}

impl TrayBridgeCatalog {
    #[must_use]
    pub(crate) const fn new() -> Self {
        Self {
            records: Vec::new(),
        }
    }

    pub(crate) fn apply(&mut self, record: TrayBridgeRecord) {
        let existing = self
            .records
            .iter()
            .position(|candidate| candidate.same_identity(&record));
        if let Some(index) = existing {
            self.records[index].merge_from(record);
        } else {
            self.records.push(record);
        }
    }

    #[must_use]
    pub(crate) fn records(&self) -> &[TrayBridgeRecord] {
        &self.records
    }
}

#[cfg(test)]
mod tests {
    use super::{TrayBridgeCatalog, TrayBridgeIdentity, TrayBridgeRecord};

    fn identity(owner_window: isize, icon_id: u32) -> TrayBridgeIdentity {
        TrayBridgeIdentity {
            owner_window,
            icon_id,
            guid: None,
        }
    }

    fn record(callback_message: u32) -> TrayBridgeRecord {
        TrayBridgeRecord::new(identity(0x1234, 7), callback_message, 4, "Radeon Software")
    }

    #[test]
    fn partial_modify_preserves_the_registered_callback() {
        let mut catalog = TrayBridgeCatalog::new();
        catalog.apply(record(0x8001));
        catalog.apply(record(0));

        let entries = catalog.records();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].icon_id(), 7);
        assert_eq!(entries[0].callback_message(), 0x8001);
        assert_eq!(entries[0].version(), 4);
        assert_eq!(entries[0].guid(), None);
        assert_eq!(entries[0].tooltip(), "Radeon Software");
    }

    #[test]
    fn set_version_updates_without_discarding_registration_data() {
        let mut catalog = TrayBridgeCatalog::new();
        catalog.apply(record(0x8001));
        catalog.apply(TrayBridgeRecord::new(identity(0x1234, 7), 0, 3, ""));

        assert_eq!(catalog.records()[0].callback_message(), 0x8001);
        assert_eq!(catalog.records()[0].version(), 3);
        assert_eq!(catalog.records()[0].tooltip(), "Radeon Software");
    }
}
