//! The `Transaction` entity and its dispute status.

use super::{Amount, Funds, Rejection, TxId};

/// The type of the funds change.
#[derive(Debug, Clone, Copy)]
enum Kind {
    Deposit,
    Withdrawal,
}

/// The position of a transaction in the dispute sequence.
#[derive(Debug, Clone, Copy)]
enum DisputeStatus {
    Undisputed,
    Disputed,
    ChargedBack,
}

/// The funds that an event adds to an account. The total changes by the sum of the two values.
#[derive(Debug, Clone, Copy)]
pub(super) struct Movement {
    pub available: Funds,
    pub held: Funds,
}

/// A deposit or a withdrawal that an account accepted.
#[derive(Debug, Clone, Copy)]
pub(super) struct Transaction {
    id: TxId,
    kind: Kind,
    amount: Amount,
    status: DisputeStatus,
}

impl Transaction {
    /// Makes the record of a deposit.
    pub fn deposit(id: TxId, amount: Amount) -> Self {
        Self::applied(id, Kind::Deposit, amount)
    }

    /// Makes the record of a withdrawal.
    pub fn withdrawal(id: TxId, amount: Amount) -> Self {
        Self::applied(id, Kind::Withdrawal, amount)
    }

    fn applied(id: TxId, kind: Kind, amount: Amount) -> Self {
        Self {
            id,
            kind,
            amount,
            status: DisputeStatus::Undisputed,
        }
    }

    /// Gives the disputed transaction and the funds change. The account uses these values only
    /// if the funds change is in range.
    pub fn dispute(self) -> Result<(Self, Movement), Rejection> {
        match self.status {
            DisputeStatus::Undisputed => {}
            DisputeStatus::Disputed => return Err(Rejection::AlreadyDisputed(self.id)),
            DisputeStatus::ChargedBack => return Err(Rejection::AlreadyChargedBack(self.id)),
        }
        // The money from a withdrawal is not in the account. Thus the dispute adds a credit to
        // the held funds, and it does not change the available funds.
        let available = match self.kind {
            Kind::Deposit => self.amount.debit(),
            Kind::Withdrawal => Funds::ZERO,
        };
        Ok((
            self.with_status(DisputeStatus::Disputed),
            Movement {
                available,
                held: self.amount.credit(),
            },
        ))
    }

    /// Gives the resolved transaction and the funds change. Only a chargeback is final, so this
    /// transaction can have a new dispute.
    pub fn resolve(self) -> Result<(Self, Movement), Rejection> {
        self.ensure_disputed()?;
        let available = match self.kind {
            Kind::Deposit => self.amount.credit(),
            Kind::Withdrawal => Funds::ZERO,
        };
        Ok((
            self.with_status(DisputeStatus::Undisputed),
            Movement {
                available,
                held: self.amount.debit(),
            },
        ))
    }

    /// Gives the charged-back transaction and the funds change.
    pub fn chargeback(self) -> Result<(Self, Movement), Rejection> {
        self.ensure_disputed()?;
        // A chargeback removes a deposit. It gives the money of a withdrawal to the client.
        let available = match self.kind {
            Kind::Deposit => Funds::ZERO,
            Kind::Withdrawal => self.amount.credit(),
        };
        Ok((
            self.with_status(DisputeStatus::ChargedBack),
            Movement {
                available,
                held: self.amount.debit(),
            },
        ))
    }

    /// Rejects this transaction if it has no open dispute.
    fn ensure_disputed(self) -> Result<(), Rejection> {
        match self.status {
            DisputeStatus::Disputed => Ok(()),
            DisputeStatus::Undisputed => Err(Rejection::NotDisputed(self.id)),
            DisputeStatus::ChargedBack => Err(Rejection::AlreadyChargedBack(self.id)),
        }
    }

    fn with_status(self, status: DisputeStatus) -> Self {
        Self { status, ..self }
    }
}
