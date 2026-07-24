#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BrightnessRequest {
    pub(crate) id: u64,
    pub(crate) value: u8,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct BrightnessApplyResult {
    pub(crate) value: u8,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("{message}")]
pub(crate) struct BrightnessApplyError {
    message: String,
}

impl BrightnessApplyError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}
#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct BrightnessCommandCoordinator {
    confirmed: u8,
    desired: u8,
    in_flight: Option<BrightnessRequest>,
    next_id: u64,
    failed: bool,
}

impl BrightnessCommandCoordinator {
    pub(crate) const fn new(confirmed: u8) -> Self {
        Self {
            confirmed,
            desired: confirmed,
            in_flight: None,
            next_id: 0,
            failed: false,
        }
    }

    pub(crate) fn sync_confirmed(&mut self, value: u8) {
        if self.in_flight.is_none() {
            self.confirmed = value.min(100);
            self.desired = self.confirmed;
            self.failed = false;
        }
    }

    pub(crate) fn request_value(&mut self, value: u8) -> Option<BrightnessRequest> {
        self.desired = value.min(100);
        self.failed = false;
        self.start_if_needed()
    }

    pub(crate) fn complete(
        &mut self,
        id: u64,
        result: Result<BrightnessApplyResult, BrightnessApplyError>,
    ) -> Option<BrightnessRequest> {
        let in_flight = self.in_flight?;
        if in_flight.id != id {
            return None;
        }
        self.in_flight = None;
        match result {
            Ok(applied) => {
                self.confirmed = applied.value.min(100);
                self.failed = false;
            }
            Err(_) => {
                self.desired = self.confirmed;
                self.failed = true;
            }
        }
        self.start_if_needed()
    }

    pub(crate) const fn visible_value(&self) -> u8 {
        self.desired
    }

    pub(crate) const fn detail(&self) -> &'static str {
        if self.failed {
            "Could not change brightness"
        } else {
            "Display brightness"
        }
    }

    fn start_if_needed(&mut self) -> Option<BrightnessRequest> {
        if self.in_flight.is_some() || self.desired == self.confirmed {
            return None;
        }
        self.next_id = self.next_id.saturating_add(1).max(1);
        let request = BrightnessRequest {
            id: self.next_id,
            value: self.desired,
        };
        self.in_flight = Some(request);
        Some(request)
    }
}

#[cfg(test)]
mod tests {
    use super::{BrightnessApplyError, BrightnessApplyResult, BrightnessCommandCoordinator};

    #[test]
    fn rapid_drags_keep_one_write_in_flight_and_coalesce_to_the_latest_value() {
        let mut coordinator = BrightnessCommandCoordinator::new(35);
        let first = coordinator.request_value(50).expect("first write");
        assert_eq!(coordinator.request_value(68), None);
        assert_eq!(coordinator.request_value(82), None);

        let next = coordinator
            .complete(first.id, Ok(BrightnessApplyResult { value: 50 }))
            .expect("coalesced write");

        assert_eq!(next.value, 82);
    }

    #[test]
    fn failed_write_restores_the_confirmed_brightness() {
        let mut coordinator = BrightnessCommandCoordinator::new(35);
        let request = coordinator.request_value(75).expect("write");

        assert_eq!(
            coordinator.complete(request.id, Err(BrightnessApplyError::new("DDC failed"))),
            None
        );
        assert_eq!(coordinator.visible_value(), 35);
        assert_eq!(coordinator.detail(), "Could not change brightness");
    }
}
