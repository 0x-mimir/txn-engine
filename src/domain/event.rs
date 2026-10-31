//! The events that a client can send.

use super::{Amount, ClientId, TxId};

/// An event on a client account. The engine uses the events in the sequence of the input.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionEvent {
    /// Adds money to the account.
    Deposit {
        /// The client that sent the event.
        client: ClientId,
        /// The ID of this new transaction.
        tx: TxId,
        /// The quantity of money.
        amount: Amount,
    },
    /// Removes money from the account.
    Withdrawal {
        /// The client that sent the event.
        client: ClientId,
        /// The ID of this new transaction.
        tx: TxId,
        /// The quantity of money.
        amount: Amount,
    },
    /// Holds the funds of an earlier transaction.
    Dispute {
        /// The client that sent the event.
        client: ClientId,
        /// The ID of the disputed transaction.
        tx: TxId,
    },
    /// Stops an open dispute.
    Resolve {
        /// The client that sent the event.
        client: ClientId,
        /// The ID of the disputed transaction.
        tx: TxId,
    },
    /// Reverses a disputed transaction and makes the account frozen.
    Chargeback {
        /// The client that sent the event.
        client: ClientId,
        /// The ID of the disputed transaction.
        tx: TxId,
    },
}
