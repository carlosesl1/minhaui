use shell_core::{
    ArmRequestId, RecoveryTransactionId, TaskbarStateFingerprint, WatchdogControlFrame,
};

/// Pure coordinator for the V1 watchdog-owned mutation lease.
///
/// Persistence is intentionally a two-step operation. `request_*` validates
/// the proposed transition without changing state; callers must finish the
/// durable journal operation before calling the matching `commit_*` method.
/// This makes it impossible for the coordinator to emit `ARMED` or `LEASE`
/// before the corresponding journal write has completed successfully.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct TaskbarArmingCoordinator {
    phase: TaskbarArmingPhase,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum TaskbarArmingPhase {
    Idle,
    Prepared {
        request_id: ArmRequestId,
        transaction_id: RecoveryTransactionId,
        fingerprint: TaskbarStateFingerprint,
    },
    Leased {
        transaction_id: RecoveryTransactionId,
    },
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct PrepareTaskbarArm {
    pub request_id: ArmRequestId,
    pub fingerprint: TaskbarStateFingerprint,
}

impl TaskbarArmingCoordinator {
    #[must_use]
    pub const fn idle() -> Self {
        Self {
            phase: TaskbarArmingPhase::Idle,
        }
    }

    #[must_use]
    pub const fn transaction_id(self) -> Option<RecoveryTransactionId> {
        match self.phase {
            TaskbarArmingPhase::Idle => None,
            TaskbarArmingPhase::Prepared { transaction_id, .. }
            | TaskbarArmingPhase::Leased { transaction_id } => Some(transaction_id),
        }
    }

    #[must_use]
    pub const fn is_leased(self) -> bool {
        matches!(self.phase, TaskbarArmingPhase::Leased { .. })
    }

    pub const fn request_arm(
        self,
        request_id: ArmRequestId,
        fingerprint: TaskbarStateFingerprint,
    ) -> Result<PrepareTaskbarArm, TaskbarArmingError> {
        match self.phase {
            TaskbarArmingPhase::Idle => Ok(PrepareTaskbarArm {
                request_id,
                fingerprint,
            }),
            TaskbarArmingPhase::Prepared { .. } | TaskbarArmingPhase::Leased { .. } => {
                Err(TaskbarArmingError::Busy)
            }
        }
    }

    /// Adopts `Prepared` only after its journal is durably visible.
    pub fn commit_prepared(
        &mut self,
        preparation: PrepareTaskbarArm,
        transaction_id: RecoveryTransactionId,
    ) -> Result<WatchdogControlFrame, TaskbarArmingError> {
        if self.phase != TaskbarArmingPhase::Idle {
            return Err(TaskbarArmingError::Busy);
        }
        self.phase = TaskbarArmingPhase::Prepared {
            request_id: preparation.request_id,
            transaction_id,
            fingerprint: preparation.fingerprint,
        };
        Ok(WatchdogControlFrame::Armed {
            request_id: preparation.request_id,
            transaction_id,
        })
    }

    /// Validates the child's commit request without granting a lease yet.
    pub fn request_lease(
        self,
        transaction_id: RecoveryTransactionId,
    ) -> Result<RecoveryTransactionId, TaskbarArmingError> {
        match self.phase {
            TaskbarArmingPhase::Prepared {
                transaction_id: found,
                ..
            } if found == transaction_id => Ok(found),
            TaskbarArmingPhase::Prepared {
                transaction_id: found,
                ..
            }
            | TaskbarArmingPhase::Leased {
                transaction_id: found,
            } => Err(TaskbarArmingError::TransactionMismatch {
                expected: found,
                found: transaction_id,
            }),
            TaskbarArmingPhase::Idle => Err(TaskbarArmingError::UnexpectedState),
        }
    }

    /// Returns the exact observation committed with this `Prepared` journal.
    ///
    /// The watchdog uses it for the second independent observation immediately
    /// before transitioning the journal to `Applied`. Keeping the digest in the
    /// coordinator ties that comparison to the authenticated transaction
    /// instead of mutable backend-local state.
    pub fn prepared_fingerprint(
        self,
        transaction_id: RecoveryTransactionId,
    ) -> Result<TaskbarStateFingerprint, TaskbarArmingError> {
        match self.phase {
            TaskbarArmingPhase::Prepared {
                transaction_id: found,
                fingerprint,
                ..
            } if found == transaction_id => Ok(fingerprint),
            TaskbarArmingPhase::Prepared {
                transaction_id: found,
                ..
            }
            | TaskbarArmingPhase::Leased {
                transaction_id: found,
            } => Err(TaskbarArmingError::TransactionMismatch {
                expected: found,
                found: transaction_id,
            }),
            TaskbarArmingPhase::Idle => Err(TaskbarArmingError::UnexpectedState),
        }
    }

    /// Grants the lease only after `Prepared -> Applied` is durable.
    pub fn commit_leased(
        &mut self,
        transaction_id: RecoveryTransactionId,
    ) -> Result<WatchdogControlFrame, TaskbarArmingError> {
        self.request_lease(transaction_id)?;
        self.phase = TaskbarArmingPhase::Leased { transaction_id };
        Ok(WatchdogControlFrame::Lease { transaction_id })
    }

    pub fn request_cancel(
        self,
        request_id: ArmRequestId,
    ) -> Result<RecoveryTransactionId, TaskbarArmingError> {
        match self.phase {
            TaskbarArmingPhase::Prepared {
                request_id: found,
                transaction_id,
                ..
            } if found == request_id => Ok(transaction_id),
            TaskbarArmingPhase::Prepared {
                request_id: found, ..
            } => Err(TaskbarArmingError::RequestMismatch {
                expected: found,
                found: request_id,
            }),
            TaskbarArmingPhase::Idle | TaskbarArmingPhase::Leased { .. } => {
                Err(TaskbarArmingError::UnexpectedState)
            }
        }
    }

    pub fn request_disarm(
        self,
        transaction_id: RecoveryTransactionId,
    ) -> Result<RecoveryTransactionId, TaskbarArmingError> {
        match self.phase {
            TaskbarArmingPhase::Prepared {
                transaction_id: found,
                ..
            }
            | TaskbarArmingPhase::Leased {
                transaction_id: found,
            } if found == transaction_id => Ok(found),
            TaskbarArmingPhase::Prepared {
                transaction_id: found,
                ..
            }
            | TaskbarArmingPhase::Leased {
                transaction_id: found,
            } => Err(TaskbarArmingError::TransactionMismatch {
                expected: found,
                found: transaction_id,
            }),
            TaskbarArmingPhase::Idle => Err(TaskbarArmingError::UnexpectedState),
        }
    }

    /// Clears ownership only after verified restoration removed the journal.
    pub fn commit_disarmed(
        &mut self,
        transaction_id: RecoveryTransactionId,
    ) -> Result<WatchdogControlFrame, TaskbarArmingError> {
        self.request_disarm(transaction_id)?;
        self.phase = TaskbarArmingPhase::Idle;
        Ok(WatchdogControlFrame::Disarmed { transaction_id })
    }
}

impl Default for TaskbarArmingCoordinator {
    fn default() -> Self {
        Self::idle()
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum TaskbarArmingError {
    #[error("another taskbar recovery transaction is already active")]
    Busy,
    #[error("the arming frame is invalid for the current state")]
    UnexpectedState,
    #[error("arming request id mismatch: expected {expected}, found {found}")]
    RequestMismatch {
        expected: ArmRequestId,
        found: ArmRequestId,
    },
    #[error("recovery transaction mismatch: expected {expected}, found {found}")]
    TransactionMismatch {
        expected: RecoveryTransactionId,
        found: RecoveryTransactionId,
    },
}

#[cfg(test)]
mod tests {
    use super::{TaskbarArmingCoordinator, TaskbarArmingError};
    use shell_core::{
        ArmRequestId, RecoveryTransactionId, TaskbarStateFingerprint, WatchdogControlFrame,
    };

    fn request(value: u64) -> ArmRequestId {
        ArmRequestId::try_new(value).expect("non-zero request id")
    }

    fn transaction(value: u8) -> RecoveryTransactionId {
        RecoveryTransactionId::try_from_bytes([value; 16]).expect("non-zero transaction id")
    }

    #[test]
    fn replies_are_unavailable_until_each_durable_commit() {
        let mut coordinator = TaskbarArmingCoordinator::idle();
        let preparation = coordinator
            .request_arm(request(1), TaskbarStateFingerprint::from_bytes([7; 32]))
            .expect("idle coordinator accepts request");

        assert_eq!(coordinator.transaction_id(), None);
        let armed = coordinator
            .commit_prepared(preparation, transaction(2))
            .expect("durable Prepared can be adopted");
        assert_eq!(
            armed,
            WatchdogControlFrame::Armed {
                request_id: request(1),
                transaction_id: transaction(2),
            }
        );

        let pending = coordinator
            .request_lease(transaction(2))
            .expect("matching child commit is accepted");
        assert_eq!(pending, transaction(2));
        assert_eq!(coordinator.transaction_id(), Some(transaction(2)));

        let lease = coordinator
            .commit_leased(transaction(2))
            .expect("durable Applied can grant a lease");
        assert_eq!(
            lease,
            WatchdogControlFrame::Lease {
                transaction_id: transaction(2)
            }
        );
        assert!(coordinator.is_leased());
    }

    #[test]
    fn mismatched_transaction_never_acquires_or_releases_a_lease() {
        let mut coordinator = TaskbarArmingCoordinator::idle();
        let preparation = coordinator
            .request_arm(request(3), TaskbarStateFingerprint::from_bytes([9; 32]))
            .expect("arm request");
        let _armed = coordinator
            .commit_prepared(preparation, transaction(4))
            .expect("prepared");

        assert_eq!(
            coordinator.request_lease(transaction(5)),
            Err(TaskbarArmingError::TransactionMismatch {
                expected: transaction(4),
                found: transaction(5),
            })
        );
        assert_eq!(
            coordinator.request_disarm(transaction(5)),
            Err(TaskbarArmingError::TransactionMismatch {
                expected: transaction(4),
                found: transaction(5),
            })
        );
        assert_eq!(coordinator.transaction_id(), Some(transaction(4)));
    }

    #[test]
    fn coordinator_clears_only_after_verified_restore_commit() {
        let mut coordinator = TaskbarArmingCoordinator::idle();
        let preparation = coordinator
            .request_arm(request(6), TaskbarStateFingerprint::from_bytes([3; 32]))
            .expect("arm request");
        let _armed = coordinator
            .commit_prepared(preparation, transaction(7))
            .expect("prepared");
        let _lease = coordinator.commit_leased(transaction(7)).expect("leased");

        assert_eq!(
            coordinator.request_disarm(transaction(7)),
            Ok(transaction(7))
        );
        assert_eq!(coordinator.transaction_id(), Some(transaction(7)));
        let response = coordinator
            .commit_disarmed(transaction(7))
            .expect("restore commit");
        assert_eq!(
            response,
            WatchdogControlFrame::Disarmed {
                transaction_id: transaction(7)
            }
        );
        assert_eq!(coordinator.transaction_id(), None);
    }
}
