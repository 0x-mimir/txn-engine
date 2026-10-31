//! The reasons for which the domain rejects an event.

use super::{ClientId, TxId};

/// The reason for which the domain rejected an event. A rejected event does not change funds.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum Rejection {
    /// The client used this transaction ID before.
    #[error("transaction {} was already used", .0.0)]
    DuplicateTransaction(TxId),
    /// The transaction ID belongs to a different client. The first client that uses an ID owns
    /// it, thus `actual` is not proof of a wrong action.
    #[error("transaction {} expected client {} but got client {}", .tx.0, .expected.0, .actual.0)]
    ClientMismatch {
        /// The ID of the transaction.
        tx: TxId,
        /// The client that owns the transaction.
        expected: ClientId,
        /// The client that sent the event.
        actual: ClientId,
    },
    /// The withdrawal is more than the available funds.
    #[error("insufficient available funds")]
    InsufficientFunds,
    /// The client has no accepted transaction with this ID.
    #[error("transaction {} is unknown to this client", .0.0)]
    UnknownTransaction(TxId),
    /// The transaction has an open dispute.
    #[error("transaction {} is already disputed", .0.0)]
    AlreadyDisputed(TxId),
    /// The transaction has no open dispute.
    #[error("transaction {} is not disputed", .0.0)]
    NotDisputed(TxId),
    /// The transaction had a chargeback. A chargeback is final.
    #[error("transaction {} was already charged back", .0.0)]
    AlreadyChargedBack(TxId),
    /// A frozen account cannot withdraw.
    #[error("account is frozen")]
    AccountFrozen,
    /// A value of the account is out of range after this event.
    #[error("balance would exceed the representable range")]
    BalanceOverflow,
}
