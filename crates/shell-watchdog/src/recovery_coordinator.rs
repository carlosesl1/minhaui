use crate::{RecoveryJournalPhase, RecoveryJournalV1};
use shell_core::{FixedHexError, RecoveryTransactionId};

/// Pure lifecycle state for one taskbar recovery transaction.
///
/// This type intentionally performs no I/O, process supervision, IPC, or
/// taskbar mutation. Callers persist the corresponding journal transition
/// before adopting the returned state.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum RecoveryCoordinator {
    Idle,
    Prepared {
        transaction_id: RecoveryTransactionId,
    },
    Applied {
        transaction_id: RecoveryTransactionId,
    },
    Restoring {
        transaction_id: RecoveryTransactionId,
    },
}

impl RecoveryCoordinator {
    #[must_use]
    pub const fn idle() -> Self {
        Self::Idle
    }

    /// Reconstructs the durable portion of the state machine from a journal.
    pub fn from_journal(
        journal: Option<&RecoveryJournalV1>,
    ) -> Result<Self, RecoveryCoordinatorError> {
        let Some(journal) = journal else {
            return Ok(Self::Idle);
        };
        let transaction_id = journal
            .transaction_id
            .parse()
            .map_err(|source| RecoveryCoordinatorError::InvalidJournalTransactionId { source })?;
        Ok(match journal.phase {
            RecoveryJournalPhase::Prepared => Self::Prepared { transaction_id },
            RecoveryJournalPhase::Applied => Self::Applied { transaction_id },
        })
    }

    #[must_use]
    pub const fn transaction_id(self) -> Option<RecoveryTransactionId> {
        match self {
            Self::Idle => None,
            Self::Prepared { transaction_id }
            | Self::Applied { transaction_id }
            | Self::Restoring { transaction_id } => Some(transaction_id),
        }
    }

    pub fn prepare(
        self,
        transaction_id: RecoveryTransactionId,
    ) -> Result<Self, RecoveryCoordinatorError> {
        match self {
            Self::Idle => Ok(Self::Prepared { transaction_id }),
            state => Err(state.unexpected("prepare", "idle")),
        }
    }

    pub fn mark_applied(
        self,
        expected_transaction_id: RecoveryTransactionId,
    ) -> Result<Self, RecoveryCoordinatorError> {
        match self {
            Self::Prepared { transaction_id } => {
                authenticate(transaction_id, expected_transaction_id)?;
                Ok(Self::Applied { transaction_id })
            }
            state => Err(state.unexpected("mark applied", "prepared")),
        }
    }

    /// Enters the transient restore state for either a prepared or applied
    /// journal. A process crash while restoring simply reconstructs the prior
    /// durable phase from the still-present journal.
    pub fn begin_restore(
        self,
        expected_transaction_id: RecoveryTransactionId,
    ) -> Result<Self, RecoveryCoordinatorError> {
        match self {
            Self::Prepared { transaction_id } | Self::Applied { transaction_id } => {
                authenticate(transaction_id, expected_transaction_id)?;
                Ok(Self::Restoring { transaction_id })
            }
            state => Err(state.unexpected("begin restore", "prepared or applied")),
        }
    }

    /// Completes restore only for the authenticated transaction. Journal
    /// deletion must succeed before a caller adopts the returned idle state.
    pub fn finish_restore(
        self,
        expected_transaction_id: RecoveryTransactionId,
    ) -> Result<Self, RecoveryCoordinatorError> {
        match self {
            Self::Restoring { transaction_id } => {
                authenticate(transaction_id, expected_transaction_id)?;
                Ok(Self::Idle)
            }
            state => Err(state.unexpected("finish restore", "restoring")),
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Idle => "idle",
            Self::Prepared { .. } => "prepared",
            Self::Applied { .. } => "applied",
            Self::Restoring { .. } => "restoring",
        }
    }

    fn unexpected(
        self,
        operation: &'static str,
        expected: &'static str,
    ) -> RecoveryCoordinatorError {
        RecoveryCoordinatorError::UnexpectedState {
            operation,
            expected,
            found: self.name(),
        }
    }
}

impl Default for RecoveryCoordinator {
    fn default() -> Self {
        Self::idle()
    }
}

fn authenticate(
    found: RecoveryTransactionId,
    expected: RecoveryTransactionId,
) -> Result<(), RecoveryCoordinatorError> {
    if found == expected {
        Ok(())
    } else {
        Err(RecoveryCoordinatorError::TransactionMismatch { expected, found })
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq, thiserror::Error)]
pub enum RecoveryCoordinatorError {
    #[error("recovery journal contains a non-canonical transaction id: {source}")]
    InvalidJournalTransactionId {
        #[source]
        source: FixedHexError,
    },
    #[error("recovery coordinator expected transaction {expected}, but found {found}")]
    TransactionMismatch {
        expected: RecoveryTransactionId,
        found: RecoveryTransactionId,
    },
    #[error("cannot {operation} while recovery coordinator is {found}; expected {expected}")]
    UnexpectedState {
        operation: &'static str,
        expected: &'static str,
        found: &'static str,
    },
}
