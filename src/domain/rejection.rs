use super::TxId;

/// Why the domain refused a transaction. Rejected transactions leave all funds unchanged.
#[derive(Debug, PartialEq, Eq, thiserror::Error)]
pub enum Rejection {
    #[error("transaction {} was already used", .0.0)]
    DuplicateTransaction(TxId),
    #[error("insufficient available funds")]
    InsufficientFunds,
    #[error("transaction {} is unknown to this client", .0.0)]
    UnknownTransaction(TxId),
    #[error("transaction {} is already disputed", .0.0)]
    AlreadyDisputed(TxId),
    #[error("transaction {} is not disputed", .0.0)]
    NotDisputed(TxId),
    #[error("transaction {} was already charged back", .0.0)]
    AlreadyChargedBack(TxId),
    #[error("account is frozen")]
    AccountFrozen,
    #[error("balance would exceed the representable range")]
    BalanceOverflow,
}
