#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct NightLightRequest {
    pub(crate) id: u64,
    pub(crate) active: bool,
    pub(crate) minimum_timestamp: u64,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct NightLightApplyResult {
    pub(crate) active: bool,
    pub(crate) timestamp: u64,
}

#[derive(Clone, Debug, Eq, PartialEq, thiserror::Error)]
#[error("{message}")]
pub(crate) struct NightLightApplyError {
    message: String,
}

impl NightLightApplyError {
    pub(crate) fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub(crate) struct NightLightCommandCoordinator {
    confirmed: bool,
    desired: bool,
    in_flight: Option<NightLightRequest>,
    next_id: u64,
    last_timestamp: u64,
    failed: bool,
}

impl NightLightCommandCoordinator {
    pub(crate) const fn new(confirmed: bool, last_timestamp: u64) -> Self {
        Self {
            confirmed,
            desired: confirmed,
            in_flight: None,
            next_id: 0,
            last_timestamp,
            failed: false,
        }
    }

    pub(crate) fn sync_confirmed(&mut self, confirmed: bool) {
        if self.in_flight.is_none() {
            self.confirmed = confirmed;
            self.desired = confirmed;
            self.failed = false;
        }
    }

    pub(crate) fn request_toggle(&mut self) -> Option<NightLightRequest> {
        self.desired = !self.desired;
        self.failed = false;
        self.start_if_needed()
    }

    pub(crate) fn complete(
        &mut self,
        id: u64,
        result: Result<NightLightApplyResult, NightLightApplyError>,
    ) -> Option<NightLightRequest> {
        let in_flight = self.in_flight?;
        if in_flight.id != id {
            return None;
        }
        self.in_flight = None;
        match result {
            Ok(applied) => {
                self.confirmed = applied.active;
                self.last_timestamp = self.last_timestamp.max(applied.timestamp);
                self.failed = false;
            }
            Err(_) => {
                self.desired = self.confirmed;
                self.failed = true;
            }
        }
        self.start_if_needed()
    }

    pub(crate) const fn confirmed(&self) -> bool {
        self.confirmed
    }

    pub(crate) const fn detail(&self) -> &'static str {
        if self.failed {
            "Could not change Night light"
        } else if self.in_flight.is_some() && self.desired {
            "Turning on..."
        } else if self.in_flight.is_some() {
            "Turning off..."
        } else if self.confirmed {
            "On"
        } else {
            "Off"
        }
    }

    fn start_if_needed(&mut self) -> Option<NightLightRequest> {
        if self.in_flight.is_some() || self.desired == self.confirmed {
            return None;
        }
        self.next_id = self.next_id.saturating_add(1).max(1);
        let request = NightLightRequest {
            id: self.next_id,
            active: self.desired,
            minimum_timestamp: self.last_timestamp,
        };
        self.in_flight = Some(request);
        Some(request)
    }
}

#[cfg(test)]
mod tests {
    use super::{NightLightApplyError, NightLightApplyResult, NightLightCommandCoordinator};

    #[test]
    fn rapid_clicks_keep_one_request_in_flight_and_latest_desired_state() {
        let mut coordinator = NightLightCommandCoordinator::new(false, 10);
        let first = coordinator.request_toggle().expect("first request");
        assert!(first.active);
        assert_eq!(coordinator.request_toggle(), None);
        assert_eq!(coordinator.request_toggle(), None);
        assert_eq!(coordinator.detail(), "Turning on...");

        let next = coordinator.complete(
            first.id,
            Ok(NightLightApplyResult {
                active: true,
                timestamp: 11,
            }),
        );

        assert_eq!(next, None);
        assert!(coordinator.confirmed());
    }

    #[test]
    fn reversing_desired_state_queues_one_serialized_follow_up() {
        let mut coordinator = NightLightCommandCoordinator::new(false, 2);
        let first = coordinator.request_toggle().expect("first request");
        assert_eq!(coordinator.request_toggle(), None);

        let second = coordinator
            .complete(
                first.id,
                Ok(NightLightApplyResult {
                    active: true,
                    timestamp: 3,
                }),
            )
            .expect("follow up");

        assert!(!second.active);
        assert_eq!(second.minimum_timestamp, 3);
    }

    #[test]
    fn failure_preserves_confirmed_state_and_exposes_concise_error() {
        let mut coordinator = NightLightCommandCoordinator::new(false, 0);
        let first = coordinator.request_toggle().expect("first request");

        assert_eq!(
            coordinator.complete(first.id, Err(NightLightApplyError::new("registry"))),
            None
        );
        assert!(!coordinator.confirmed());
        assert_eq!(coordinator.detail(), "Could not change Night light");
    }
}
