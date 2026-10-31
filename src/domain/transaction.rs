use super::{Amount, Funds, Rejection, TxId};

#[derive(Debug, Clone, Copy)]
enum Kind {
    Deposit,
    Withdrawal,
}

#[derive(Debug, Clone, Copy)]
enum DisputeStatus {
    Undisputed,
    Disputed,
    ChargedBack,
}

/// Funds to shift on an account; total changes by the sum of both.
#[derive(Debug, Clone, Copy)]
pub(super) struct Movement {
    pub available: Funds,
    pub held: Funds,
}

/// A deposit or withdrawal an account has applied, which stays open to dispute.
#[derive(Debug, Clone, Copy)]
pub(super) struct Transaction {
    id: TxId,
    kind: Kind,
    amount: Amount,
    status: DisputeStatus,
}

impl Transaction {
    pub fn deposit(id: TxId, amount: Amount) -> Self {
        Self::applied(id, Kind::Deposit, amount)
    }

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

    pub fn dispute(self) -> Result<(Self, Movement), Rejection> {
        match self.status {
            DisputeStatus::Undisputed => {}
            DisputeStatus::Disputed => return Err(Rejection::AlreadyDisputed(self.id)),
            DisputeStatus::ChargedBack => return Err(Rejection::AlreadyChargedBack(self.id)),
        }
        // A withdrawn amount left the account, so its hold is a credit on top of available.
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

    /// Undoes the dispute; only a chargeback is final, so this one may be disputed again.
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

    pub fn chargeback(self) -> Result<(Self, Movement), Rejection> {
        self.ensure_disputed()?;
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
