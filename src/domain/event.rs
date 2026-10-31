use super::{Amount, ClientId, TxId};

/// Something that happened to a client's account, in the order it must be applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TransactionEvent {
    Deposit {
        client: ClientId,
        tx: TxId,
        amount: Amount,
    },
    Withdrawal {
        client: ClientId,
        tx: TxId,
        amount: Amount,
    },
    Dispute {
        client: ClientId,
        tx: TxId,
    },
    Resolve {
        client: ClientId,
        tx: TxId,
    },
    Chargeback {
        client: ClientId,
        tx: TxId,
    },
}
