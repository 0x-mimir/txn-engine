use std::collections::HashMap;

use super::transaction::{Movement, Transaction};
use super::{Amount, ClientId, Funds, Rejection, TxId};

/// Aggregate root: a client's funds plus the transactions that can still be disputed.
/// Frozen accounts refuse only withdrawals, so a negative available can still be repaid.
#[derive(Debug)]
pub struct Account {
    client: ClientId,
    available: Funds,
    held: Funds,
    frozen: bool,
    transactions: HashMap<TxId, Transaction>,
}

impl Account {
    pub fn new(client: ClientId) -> Self {
        Self {
            client,
            available: Funds::ZERO,
            held: Funds::ZERO,
            frozen: false,
            transactions: HashMap::new(),
        }
    }

    pub fn client(&self) -> ClientId {
        self.client
    }

    pub fn available(&self) -> Funds {
        self.available
    }

    pub fn held(&self) -> Funds {
        self.held
    }

    pub fn total(&self) -> Funds {
        self.available
            .checked_add(self.held)
            .expect("record() rejects movements that would overflow the total")
    }

    pub fn is_frozen(&self) -> bool {
        self.frozen
    }

    pub fn deposit(&mut self, tx: TxId, amount: Amount) -> Result<(), Rejection> {
        self.ensure_unused(tx)?;
        let movement = Movement {
            available: amount.credit(),
            held: Funds::ZERO,
        };
        self.record(tx, Transaction::deposit(tx, amount), movement)
    }

    pub fn withdraw(&mut self, tx: TxId, amount: Amount) -> Result<(), Rejection> {
        if self.frozen {
            return Err(Rejection::AccountFrozen);
        }
        self.ensure_unused(tx)?;
        if amount.credit() > self.available {
            return Err(Rejection::InsufficientFunds);
        }
        let movement = Movement {
            available: amount.debit(),
            held: Funds::ZERO,
        };
        self.record(tx, Transaction::withdrawal(tx, amount), movement)
    }

    pub fn dispute(&mut self, tx: TxId) -> Result<(), Rejection> {
        let (disputed, movement) = self.transaction(tx)?.dispute()?;
        self.record(tx, disputed, movement)
    }

    pub fn resolve(&mut self, tx: TxId) -> Result<(), Rejection> {
        let (resolved, movement) = self.transaction(tx)?.resolve()?;
        self.record(tx, resolved, movement)
    }

    pub fn chargeback(&mut self, tx: TxId) -> Result<(), Rejection> {
        let (charged_back, movement) = self.transaction(tx)?.chargeback()?;
        self.record(tx, charged_back, movement)?;
        self.frozen = true;
        Ok(())
    }

    fn ensure_unused(&self, tx: TxId) -> Result<(), Rejection> {
        match self.transactions.contains_key(&tx) {
            true => Err(Rejection::DuplicateTransaction(tx)),
            false => Ok(()),
        }
    }

    fn transaction(&self, tx: TxId) -> Result<Transaction, Rejection> {
        self.transactions
            .get(&tx)
            .copied()
            .ok_or(Rejection::UnknownTransaction(tx))
    }

    fn record(
        &mut self,
        tx: TxId,
        transaction: Transaction,
        movement: Movement,
    ) -> Result<(), Rejection> {
        let available = self
            .available
            .checked_add(movement.available)
            .ok_or(Rejection::BalanceOverflow)?;
        let held = self
            .held
            .checked_add(movement.held)
            .ok_or(Rejection::BalanceOverflow)?;
        // Checked here so that total() can add the two without panicking.
        available
            .checked_add(held)
            .ok_or(Rejection::BalanceOverflow)?;
        self.available = available;
        self.held = held;
        self.transactions.insert(tx, transaction);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn amount(value: &str) -> Amount {
        value.parse().unwrap()
    }

    fn max_amount() -> Amount {
        amount("922337203685477.5807")
    }

    fn account() -> Account {
        Account::new(ClientId(1))
    }

    fn frozen_account_with_one_unit_available() -> Account {
        let mut account = account();
        account.deposit(TxId(1), amount("3.0")).unwrap();
        account.deposit(TxId(2), amount("1.0")).unwrap();
        account.dispute(TxId(1)).unwrap();
        account.chargeback(TxId(1)).unwrap();
        account
    }

    #[test]
    fn a_new_account_belongs_to_its_client_and_is_empty() {
        let account = account();

        assert_eq!(account.client(), ClientId(1));
        assert_eq!(account.total(), Funds::ZERO);
        assert!(!account.is_frozen());
    }

    #[test]
    fn deposit_increases_available_and_total() {
        let mut account = account();

        account.deposit(TxId(1), amount("1.5")).unwrap();

        assert_eq!(account.available(), Funds(1_5000));
        assert_eq!(account.held(), Funds::ZERO);
        assert_eq!(account.total(), Funds(1_5000));
    }

    #[test]
    fn withdrawal_decreases_available_and_total() {
        let mut account = account();
        account.deposit(TxId(1), amount("3.0")).unwrap();

        account.withdraw(TxId(2), amount("1.0")).unwrap();

        assert_eq!(account.available(), Funds(2_0000));
        assert_eq!(account.total(), Funds(2_0000));
    }

    #[test]
    fn withdrawal_can_empty_the_account() {
        let mut account = account();
        account.deposit(TxId(1), amount("3.0")).unwrap();

        account.withdraw(TxId(2), amount("3.0")).unwrap();

        assert_eq!(account.available(), Funds::ZERO);
    }

    #[test]
    fn withdrawal_above_available_fails_and_leaves_funds_unchanged() {
        let mut account = account();
        account.deposit(TxId(1), amount("2.0")).unwrap();

        let result = account.withdraw(TxId(2), amount("3.0"));

        assert_eq!(result, Err(Rejection::InsufficientFunds));
        assert_eq!(account.available(), Funds(2_0000));
        assert_eq!(account.total(), Funds(2_0000));
    }

    #[test]
    fn held_funds_cannot_be_withdrawn() {
        let mut account = account();
        account.deposit(TxId(1), amount("10.0")).unwrap();
        account.dispute(TxId(1)).unwrap();

        assert_eq!(
            account.withdraw(TxId(2), amount("5.0")),
            Err(Rejection::InsufficientFunds)
        );
        assert_eq!(account.held(), Funds(10_0000));
    }

    #[test]
    fn a_reused_transaction_id_is_rejected_and_leaves_the_original_disputable() {
        let mut account = account();
        account.deposit(TxId(1), amount("5.0")).unwrap();
        account.dispute(TxId(1)).unwrap();

        assert_eq!(
            account.deposit(TxId(1), amount("7.0")),
            Err(Rejection::DuplicateTransaction(TxId(1)))
        );
        assert_eq!(
            account.withdraw(TxId(1), amount("0.0001")),
            Err(Rejection::DuplicateTransaction(TxId(1)))
        );
        assert_eq!(account.total(), Funds(5_0000));

        account.resolve(TxId(1)).unwrap();
        assert_eq!(account.available(), Funds(5_0000));
    }

    #[test]
    fn a_charged_back_transaction_id_stays_used() {
        let mut account = frozen_account_with_one_unit_available();

        assert_eq!(
            account.deposit(TxId(1), amount("1.0")),
            Err(Rejection::DuplicateTransaction(TxId(1)))
        );
    }

    #[test]
    fn a_frozen_account_reports_frozen_before_a_reused_transaction_id() {
        let mut account = frozen_account_with_one_unit_available();

        assert_eq!(
            account.withdraw(TxId(2), amount("0.5")),
            Err(Rejection::AccountFrozen)
        );
    }

    #[test]
    fn a_reused_transaction_id_cannot_withdraw() {
        let mut account = account();
        account.deposit(TxId(1), amount("5.0")).unwrap();

        assert_eq!(
            account.withdraw(TxId(1), amount("1.0")),
            Err(Rejection::DuplicateTransaction(TxId(1)))
        );
        assert_eq!(account.available(), Funds(5_0000));
    }

    #[test]
    fn disputing_a_deposit_moves_the_amount_from_available_to_held() {
        let mut account = account();
        account.deposit(TxId(1), amount("3.0")).unwrap();
        account.deposit(TxId(2), amount("1.0")).unwrap();

        account.dispute(TxId(1)).unwrap();

        assert_eq!(account.available(), Funds(1_0000));
        assert_eq!(account.held(), Funds(3_0000));
        assert_eq!(account.total(), Funds(4_0000));
    }

    #[test]
    fn disputing_an_already_spent_deposit_leaves_available_negative() {
        let mut account = account();
        account.deposit(TxId(1), amount("3.0")).unwrap();
        account.withdraw(TxId(2), amount("3.0")).unwrap();

        account.dispute(TxId(1)).unwrap();

        assert_eq!(account.available(), Funds(-3_0000));
        assert_eq!(account.held(), Funds(3_0000));
        assert_eq!(account.total(), Funds::ZERO);
    }

    #[test]
    fn disputing_an_unknown_transaction_is_rejected() {
        let mut account = account();
        account.deposit(TxId(1), amount("3.0")).unwrap();

        assert_eq!(
            account.dispute(TxId(9)),
            Err(Rejection::UnknownTransaction(TxId(9)))
        );
        assert_eq!(account.available(), Funds(3_0000));
    }

    #[test]
    fn a_failed_withdrawal_cannot_be_disputed() {
        let mut account = account();
        account.deposit(TxId(1), amount("1.0")).unwrap();
        account.withdraw(TxId(2), amount("5.0")).unwrap_err();

        assert_eq!(
            account.dispute(TxId(2)),
            Err(Rejection::UnknownTransaction(TxId(2)))
        );
    }

    #[test]
    fn a_disputed_transaction_cannot_be_disputed_again() {
        let mut account = account();
        account.deposit(TxId(1), amount("3.0")).unwrap();
        account.dispute(TxId(1)).unwrap();

        assert_eq!(
            account.dispute(TxId(1)),
            Err(Rejection::AlreadyDisputed(TxId(1)))
        );
        assert_eq!(account.held(), Funds(3_0000));
    }

    #[test]
    fn resolving_a_deposit_restores_the_state_before_the_dispute() {
        let mut account = account();
        account.deposit(TxId(1), amount("3.0")).unwrap();
        account.dispute(TxId(1)).unwrap();

        account.resolve(TxId(1)).unwrap();

        assert_eq!(account.available(), Funds(3_0000));
        assert_eq!(account.held(), Funds::ZERO);
        assert_eq!(account.total(), Funds(3_0000));
        assert!(!account.is_frozen());
    }

    #[test]
    fn a_resolved_transaction_can_be_disputed_again() {
        let mut account = account();
        account.deposit(TxId(1), amount("3.0")).unwrap();
        account.dispute(TxId(1)).unwrap();
        account.resolve(TxId(1)).unwrap();

        account.dispute(TxId(1)).unwrap();

        assert_eq!(account.held(), Funds(3_0000));
    }

    #[test]
    fn only_a_disputed_transaction_can_be_resolved_or_charged_back() {
        let mut account = account();
        account.deposit(TxId(1), amount("3.0")).unwrap();

        let not_disputed = Err(Rejection::NotDisputed(TxId(1)));
        assert_eq!(account.resolve(TxId(1)), not_disputed);
        assert_eq!(account.chargeback(TxId(1)), not_disputed);
        assert_eq!(account.available(), Funds(3_0000));
        assert!(!account.is_frozen());
    }

    #[test]
    fn resolve_and_chargeback_of_an_unknown_transaction_are_rejected() {
        let mut account = account();
        let unknown = Err(Rejection::UnknownTransaction(TxId(9)));

        assert_eq!(account.resolve(TxId(9)), unknown);
        assert_eq!(account.chargeback(TxId(9)), unknown);
    }

    #[test]
    fn chargeback_of_a_deposit_removes_held_funds_and_freezes_the_account() {
        let mut account = account();
        account.deposit(TxId(1), amount("3.0")).unwrap();
        account.deposit(TxId(2), amount("1.0")).unwrap();
        account.dispute(TxId(1)).unwrap();

        account.chargeback(TxId(1)).unwrap();

        assert_eq!(account.available(), Funds(1_0000));
        assert_eq!(account.held(), Funds::ZERO);
        assert_eq!(account.total(), Funds(1_0000));
        assert!(account.is_frozen());
    }

    #[test]
    fn a_charged_back_transaction_is_final() {
        let mut account = account();
        account.deposit(TxId(1), amount("3.0")).unwrap();
        account.dispute(TxId(1)).unwrap();
        account.chargeback(TxId(1)).unwrap();
        let final_state = Err(Rejection::AlreadyChargedBack(TxId(1)));

        assert_eq!(account.dispute(TxId(1)), final_state);
        assert_eq!(account.resolve(TxId(1)), final_state);
        assert_eq!(account.chargeback(TxId(1)), final_state);
        assert_eq!(account.total(), Funds::ZERO);
    }

    #[test]
    fn a_frozen_account_rejects_withdrawals_even_when_funds_are_short() {
        let mut account = frozen_account_with_one_unit_available();

        assert_eq!(
            account.withdraw(TxId(4), amount("0.5")),
            Err(Rejection::AccountFrozen)
        );
        assert_eq!(
            account.withdraw(TxId(5), amount("9.0")),
            Err(Rejection::AccountFrozen)
        );
        assert_eq!(account.total(), Funds(1_0000));
        assert_eq!(
            account.dispute(TxId(4)),
            Err(Rejection::UnknownTransaction(TxId(4)))
        );
    }

    #[test]
    fn a_frozen_account_can_reconcile_a_negative_available_through_deposits() {
        let mut account = account();
        account.deposit(TxId(1), amount("3.0")).unwrap();
        account.withdraw(TxId(2), amount("3.0")).unwrap();
        account.dispute(TxId(1)).unwrap();
        account.chargeback(TxId(1)).unwrap();
        assert_eq!(account.available(), Funds(-3_0000));

        account.deposit(TxId(3), amount("5.0")).unwrap();

        assert_eq!(account.available(), Funds(2_0000));
        assert_eq!(account.total(), Funds(2_0000));
        assert!(account.is_frozen());
    }

    #[test]
    fn a_deposit_on_a_frozen_account_can_be_disputed_and_charged_back() {
        let mut account = frozen_account_with_one_unit_available();
        account.deposit(TxId(4), amount("2.0")).unwrap();

        account.dispute(TxId(4)).unwrap();
        assert_eq!(account.held(), Funds(2_0000));

        account.chargeback(TxId(4)).unwrap();
        assert_eq!(account.total(), Funds(1_0000));
        assert!(account.is_frozen());
    }

    #[test]
    fn a_frozen_account_can_open_a_dispute_on_an_earlier_transaction() {
        let mut account = frozen_account_with_one_unit_available();

        account.dispute(TxId(2)).unwrap();

        assert_eq!(account.available(), Funds::ZERO);
        assert_eq!(account.held(), Funds(1_0000));
        assert!(account.is_frozen());
    }

    #[test]
    fn a_frozen_account_can_still_settle_open_disputes() {
        let mut account = account();
        account.deposit(TxId(1), amount("3.0")).unwrap();
        account.deposit(TxId(2), amount("1.0")).unwrap();
        account.dispute(TxId(1)).unwrap();
        account.dispute(TxId(2)).unwrap();
        account.chargeback(TxId(1)).unwrap();

        account.resolve(TxId(2)).unwrap();

        assert_eq!(account.available(), Funds(1_0000));
        assert_eq!(account.held(), Funds::ZERO);
        assert!(account.is_frozen());
    }

    #[test]
    fn a_frozen_account_can_still_charge_back_open_disputes() {
        let mut account = account();
        account.deposit(TxId(1), amount("3.0")).unwrap();
        account.deposit(TxId(2), amount("1.0")).unwrap();
        account.dispute(TxId(1)).unwrap();
        account.dispute(TxId(2)).unwrap();
        account.chargeback(TxId(1)).unwrap();

        account.chargeback(TxId(2)).unwrap();

        assert_eq!(account.total(), Funds::ZERO);
    }

    #[test]
    fn disputing_a_withdrawal_holds_a_provisional_credit() {
        let mut account = account();
        account.deposit(TxId(1), amount("10.0")).unwrap();
        account.withdraw(TxId(2), amount("3.0")).unwrap();

        account.dispute(TxId(2)).unwrap();

        assert_eq!(account.available(), Funds(7_0000));
        assert_eq!(account.held(), Funds(3_0000));
        assert_eq!(account.total(), Funds(10_0000));
    }

    #[test]
    fn resolving_a_withdrawal_drops_the_provisional_credit() {
        let mut account = account();
        account.deposit(TxId(1), amount("10.0")).unwrap();
        account.withdraw(TxId(2), amount("3.0")).unwrap();
        account.dispute(TxId(2)).unwrap();

        account.resolve(TxId(2)).unwrap();

        assert_eq!(account.available(), Funds(7_0000));
        assert_eq!(account.held(), Funds::ZERO);
        assert_eq!(account.total(), Funds(7_0000));
    }

    #[test]
    fn chargeback_of_a_withdrawal_refunds_the_client_and_freezes_the_account() {
        let mut account = account();
        account.deposit(TxId(1), amount("10.0")).unwrap();
        account.withdraw(TxId(2), amount("3.0")).unwrap();
        account.dispute(TxId(2)).unwrap();

        account.chargeback(TxId(2)).unwrap();

        assert_eq!(account.available(), Funds(10_0000));
        assert_eq!(account.held(), Funds::ZERO);
        assert_eq!(account.total(), Funds(10_0000));
        assert!(account.is_frozen());
    }

    #[test]
    fn a_deposit_that_would_overflow_is_rejected_and_not_recorded() {
        let mut account = account();
        account.deposit(TxId(1), max_amount()).unwrap();

        assert_eq!(
            account.deposit(TxId(2), amount("1.0")),
            Err(Rejection::BalanceOverflow)
        );
        assert_eq!(account.available(), Funds(i64::MAX));
        assert_eq!(
            account.dispute(TxId(2)),
            Err(Rejection::UnknownTransaction(TxId(2)))
        );
    }

    #[test]
    fn a_deposit_that_would_overflow_the_total_is_rejected() {
        let mut account = account();
        account.deposit(TxId(1), max_amount()).unwrap();
        account.dispute(TxId(1)).unwrap();

        assert_eq!(
            account.deposit(TxId(2), max_amount()),
            Err(Rejection::BalanceOverflow)
        );
        assert_eq!(account.held(), Funds(i64::MAX));
        assert_eq!(account.total(), Funds(i64::MAX));
    }

    #[test]
    fn a_dispute_that_would_overflow_held_is_rejected() {
        let mut account = account();
        account.deposit(TxId(1), max_amount()).unwrap();
        account.withdraw(TxId(2), max_amount()).unwrap();
        account.dispute(TxId(1)).unwrap();
        account.deposit(TxId(3), amount("1.0")).unwrap();

        assert_eq!(account.dispute(TxId(3)), Err(Rejection::BalanceOverflow));
        assert_eq!(account.held(), Funds(i64::MAX));
    }

    #[test]
    fn a_dispute_that_would_overflow_leaves_the_transaction_undisputed() {
        let mut account = account();
        account.deposit(TxId(1), max_amount()).unwrap();
        account.withdraw(TxId(2), amount("1.0")).unwrap();
        account.deposit(TxId(3), amount("1.0")).unwrap();

        assert_eq!(account.dispute(TxId(2)), Err(Rejection::BalanceOverflow));
        assert_eq!(account.held(), Funds::ZERO);
        assert_eq!(
            account.resolve(TxId(2)),
            Err(Rejection::NotDisputed(TxId(2)))
        );
    }
}
