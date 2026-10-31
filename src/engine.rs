use std::collections::hash_map::Entry;
use std::collections::{BTreeMap, HashMap};

use crate::domain::{Account, ClientId, Rejection, TransactionEvent, TxId};

/// Applies transaction events in order to the accounts of all clients.
#[derive(Debug, Default)]
pub struct Engine {
    accounts: BTreeMap<ClientId, Account>,
    transaction_owners: HashMap<TxId, ClientId>,
}

impl Engine {
    pub fn process(&mut self, event: TransactionEvent) -> Result<(), Rejection> {
        match event {
            TransactionEvent::Deposit { client, tx, amount } => {
                self.register_transaction(client, tx)?;
                self.open_account(client).deposit(tx, amount)
            }
            TransactionEvent::Withdrawal { client, tx, amount } => {
                self.register_transaction(client, tx)?;
                self.open_account(client).withdraw(tx, amount)
            }
            TransactionEvent::Dispute { client, tx } => self.owned_account(client, tx)?.dispute(tx),
            TransactionEvent::Resolve { client, tx } => self.owned_account(client, tx)?.resolve(tx),
            TransactionEvent::Chargeback { client, tx } => {
                self.owned_account(client, tx)?.chargeback(tx)
            }
        }
    }

    pub fn accounts(&self) -> impl Iterator<Item = &Account> {
        self.accounts.values()
    }

    /// IDs are globally unique, so even a refused transaction stays with its submitter.
    fn register_transaction(&mut self, client: ClientId, tx: TxId) -> Result<(), Rejection> {
        match self.transaction_owners.entry(tx) {
            Entry::Vacant(entry) => {
                entry.insert(client);
                Ok(())
            }
            Entry::Occupied(entry) if *entry.get() == client => {
                Err(Rejection::DuplicateTransaction(tx))
            }
            Entry::Occupied(entry) => Err(Rejection::ClientMismatch {
                tx,
                expected: *entry.get(),
                actual: client,
            }),
        }
    }

    fn owned_account(&mut self, client: ClientId, tx: TxId) -> Result<&mut Account, Rejection> {
        match self.transaction_owners.get(&tx) {
            Some(&expected) if expected != client => Err(Rejection::ClientMismatch {
                tx,
                expected,
                actual: client,
            }),
            Some(_) | None => self
                .accounts
                .get_mut(&client)
                .ok_or(Rejection::UnknownTransaction(tx)),
        }
    }

    fn open_account(&mut self, client: ClientId) -> &mut Account {
        self.accounts
            .entry(client)
            .or_insert_with(|| Account::new(client))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::{Amount, ClientId, Funds, TxId};

    fn amount(value: &str) -> Amount {
        value.parse().unwrap()
    }

    fn deposit(client: u16, tx: u32, value: &str) -> TransactionEvent {
        TransactionEvent::Deposit {
            client: ClientId(client),
            tx: TxId(tx),
            amount: amount(value),
        }
    }

    fn withdrawal(client: u16, tx: u32, value: &str) -> TransactionEvent {
        TransactionEvent::Withdrawal {
            client: ClientId(client),
            tx: TxId(tx),
            amount: amount(value),
        }
    }

    fn dispute(client: u16, tx: u32) -> TransactionEvent {
        TransactionEvent::Dispute {
            client: ClientId(client),
            tx: TxId(tx),
        }
    }

    fn resolve(client: u16, tx: u32) -> TransactionEvent {
        TransactionEvent::Resolve {
            client: ClientId(client),
            tx: TxId(tx),
        }
    }

    fn chargeback(client: u16, tx: u32) -> TransactionEvent {
        TransactionEvent::Chargeback {
            client: ClientId(client),
            tx: TxId(tx),
        }
    }

    fn client_mismatch(actual: u16, tx: u32, expected: u16) -> Result<(), Rejection> {
        Err(Rejection::ClientMismatch {
            tx: TxId(tx),
            expected: ClientId(expected),
            actual: ClientId(actual),
        })
    }

    fn account(engine: &Engine, client: u16) -> &Account {
        engine
            .accounts()
            .find(|account| account.client() == ClientId(client))
            .unwrap()
    }

    #[test]
    fn deposits_and_withdrawals_reach_their_own_client() {
        let mut engine = Engine::default();

        engine.process(deposit(1, 1, "1.0")).unwrap();
        engine.process(deposit(2, 2, "2.0")).unwrap();
        engine.process(deposit(1, 3, "2.0")).unwrap();
        engine.process(withdrawal(1, 4, "1.5")).unwrap();
        let refused = engine.process(withdrawal(2, 5, "3.0"));

        assert_eq!(refused, Err(Rejection::InsufficientFunds));
        assert_eq!(account(&engine, 1).available(), Funds(1_5000));
        assert_eq!(account(&engine, 2).available(), Funds(2_0000));
    }

    #[test]
    fn accounts_are_listed_in_client_order() {
        let mut engine = Engine::default();

        engine.process(deposit(9, 1, "1.0")).unwrap();
        engine.process(deposit(3, 2, "1.0")).unwrap();
        engine.process(deposit(5, 3, "1.0")).unwrap();

        let clients: Vec<_> = engine.accounts().map(Account::client).collect();
        assert_eq!(clients, [ClientId(3), ClientId(5), ClientId(9)]);
    }

    #[test]
    fn a_client_cannot_reuse_its_own_transaction_id() {
        let mut engine = Engine::default();
        engine.process(deposit(1, 1, "1.0")).unwrap();

        assert_eq!(
            engine.process(deposit(1, 1, "5.0")),
            Err(Rejection::DuplicateTransaction(TxId(1)))
        );
        assert_eq!(account(&engine, 1).total(), Funds(1_0000));
    }

    #[test]
    fn a_client_cannot_deposit_or_withdraw_under_another_clients_transaction_id() {
        let mut engine = Engine::default();
        engine.process(deposit(1, 1, "1.0")).unwrap();
        engine.process(deposit(2, 2, "1.0")).unwrap();

        assert_eq!(
            engine.process(deposit(2, 1, "5.0")),
            client_mismatch(2, 1, 1)
        );
        assert_eq!(
            engine.process(withdrawal(2, 1, "0.5")),
            client_mismatch(2, 1, 1)
        );
        assert_eq!(
            engine.process(deposit(3, 1, "5.0")),
            client_mismatch(3, 1, 1)
        );
        assert_eq!(
            engine.process(withdrawal(4, 1, "0.5")),
            client_mismatch(4, 1, 1)
        );
        assert_eq!(account(&engine, 2).total(), Funds(1_0000));
        assert_eq!(engine.accounts().count(), 2);
    }

    #[test]
    fn a_refused_withdrawal_still_uses_up_its_transaction_id() {
        let mut engine = Engine::default();
        engine.process(deposit(1, 1, "1.0")).unwrap();
        engine.process(withdrawal(1, 2, "5.0")).unwrap_err();

        assert_eq!(
            engine.process(withdrawal(1, 2, "0.5")),
            Err(Rejection::DuplicateTransaction(TxId(2)))
        );
        assert_eq!(account(&engine, 1).available(), Funds(1_0000));
    }

    #[test]
    fn dispute_resolve_and_chargeback_refer_to_their_transaction_by_id() {
        let mut engine = Engine::default();
        engine.process(deposit(1, 1, "3.0")).unwrap();
        engine.process(deposit(1, 2, "1.0")).unwrap();

        engine.process(dispute(1, 1)).unwrap();
        assert_eq!(account(&engine, 1).held(), Funds(3_0000));

        engine.process(resolve(1, 1)).unwrap();
        assert_eq!(account(&engine, 1).held(), Funds::ZERO);

        engine.process(dispute(1, 2)).unwrap();
        engine.process(chargeback(1, 2)).unwrap();
        assert_eq!(account(&engine, 1).total(), Funds(3_0000));
        assert!(account(&engine, 1).is_frozen());
    }

    #[test]
    fn a_client_cannot_touch_another_clients_transaction() {
        let mut engine = Engine::default();
        engine.process(deposit(1, 1, "3.0")).unwrap();
        engine.process(deposit(2, 2, "1.0")).unwrap();
        engine.process(dispute(1, 1)).unwrap();

        assert_eq!(engine.process(dispute(2, 1)), client_mismatch(2, 1, 1));
        assert_eq!(engine.process(resolve(2, 1)), client_mismatch(2, 1, 1));
        assert_eq!(engine.process(chargeback(2, 1)), client_mismatch(2, 1, 1));

        assert_eq!(account(&engine, 1).held(), Funds(3_0000));
        assert_eq!(account(&engine, 2).available(), Funds(1_0000));
        assert!(!account(&engine, 1).is_frozen());
        assert!(!account(&engine, 2).is_frozen());
    }

    #[test]
    fn a_dispute_from_the_wrong_client_moves_none_of_the_owners_funds() {
        let mut engine = Engine::default();
        engine.process(deposit(1, 1, "3.0")).unwrap();
        engine.process(deposit(2, 2, "1.0")).unwrap();

        assert_eq!(engine.process(dispute(2, 1)), client_mismatch(2, 1, 1));

        assert_eq!(account(&engine, 1).available(), Funds(3_0000));
        assert_eq!(account(&engine, 1).held(), Funds::ZERO);
        assert_eq!(account(&engine, 2).available(), Funds(1_0000));
        assert_eq!(account(&engine, 2).held(), Funds::ZERO);
    }

    #[test]
    fn a_refused_transaction_id_cannot_be_taken_over_by_another_client() {
        let mut engine = Engine::default();
        engine.process(deposit(1, 1, "5.0")).unwrap();
        engine.process(withdrawal(1, 2, "9.0")).unwrap_err();

        assert_eq!(
            engine.process(deposit(2, 2, "1.0")),
            client_mismatch(2, 2, 1)
        );
        assert_eq!(
            engine.process(withdrawal(2, 2, "1.0")),
            client_mismatch(2, 2, 1)
        );

        assert_eq!(account(&engine, 1).total(), Funds(5_0000));
        assert_eq!(engine.accounts().count(), 1);
    }

    #[test]
    fn a_deposit_refused_for_overflow_keeps_its_owner() {
        let mut engine = Engine::default();
        let largest = "922337203685477.5807";
        engine.process(deposit(1, 1, largest)).unwrap();
        engine.process(deposit(1, 2, "1.0")).unwrap_err();

        assert_eq!(
            engine.process(deposit(1, 2, "1.0")),
            Err(Rejection::DuplicateTransaction(TxId(2)))
        );
        assert_eq!(
            engine.process(deposit(2, 2, "1.0")),
            client_mismatch(2, 2, 1)
        );
    }

    #[test]
    fn a_failed_reference_event_claims_no_transaction_id() {
        let mut engine = Engine::default();
        engine.process(dispute(2, 1)).unwrap_err();
        engine.process(resolve(2, 2)).unwrap_err();
        engine.process(chargeback(2, 3)).unwrap_err();

        engine.process(deposit(1, 1, "1.0")).unwrap();
        engine.process(deposit(1, 2, "1.0")).unwrap();
        engine.process(deposit(1, 3, "1.0")).unwrap();

        assert_eq!(account(&engine, 1).total(), Funds(3_0000));
    }

    #[test]
    fn a_refused_withdrawal_opens_an_empty_account_for_a_new_client() {
        let mut engine = Engine::default();

        engine.process(withdrawal(4, 1, "5.0")).unwrap_err();

        assert_eq!(account(&engine, 4).total(), Funds::ZERO);
    }

    #[test]
    fn a_frozen_account_processes_every_event_except_withdrawals() {
        let mut engine = Engine::default();
        engine.process(deposit(1, 1, "3.0")).unwrap();
        engine.process(deposit(1, 2, "1.0")).unwrap();
        engine.process(deposit(1, 3, "1.0")).unwrap();
        engine.process(dispute(1, 1)).unwrap();
        engine.process(chargeback(1, 1)).unwrap();

        engine.process(deposit(1, 4, "2.0")).unwrap();
        engine.process(dispute(1, 2)).unwrap();
        engine.process(resolve(1, 2)).unwrap();
        engine.process(dispute(1, 3)).unwrap();
        engine.process(chargeback(1, 3)).unwrap();

        assert_eq!(account(&engine, 1).total(), Funds(3_0000));
        assert_eq!(account(&engine, 1).held(), Funds::ZERO);
        assert!(account(&engine, 1).is_frozen());
        assert_eq!(
            engine.process(withdrawal(1, 5, "0.5")),
            Err(Rejection::AccountFrozen)
        );
    }

    #[test]
    fn a_client_mismatch_names_the_expected_and_actual_client() {
        let rejection = Rejection::ClientMismatch {
            tx: TxId(50),
            expected: ClientId(2),
            actual: ClientId(1),
        };

        assert_eq!(
            rejection.to_string(),
            "transaction 50 expected client 2 but got client 1"
        );
    }

    #[test]
    fn the_owner_can_still_settle_after_an_attempt_by_another_client() {
        let mut engine = Engine::default();
        engine.process(deposit(1, 1, "3.0")).unwrap();
        engine.process(dispute(1, 1)).unwrap();
        engine.process(resolve(2, 1)).unwrap_err();

        engine.process(resolve(1, 1)).unwrap();

        assert_eq!(account(&engine, 1).available(), Funds(3_0000));
    }

    #[test]
    fn a_client_without_an_account_cannot_touch_a_transaction_or_gain_an_account() {
        let mut engine = Engine::default();
        engine.process(deposit(1, 1, "3.0")).unwrap();

        assert_eq!(engine.process(dispute(7, 1)), client_mismatch(7, 1, 1));
        assert_eq!(engine.process(resolve(7, 1)), client_mismatch(7, 1, 1));
        assert_eq!(engine.process(chargeback(7, 1)), client_mismatch(7, 1, 1));
        assert_eq!(engine.accounts().count(), 1);
    }

    #[test]
    fn a_refused_transaction_still_belongs_to_the_client_that_submitted_it() {
        let mut engine = Engine::default();
        engine.process(deposit(1, 1, "1.0")).unwrap();
        engine.process(deposit(2, 2, "1.0")).unwrap();
        engine.process(withdrawal(1, 3, "5.0")).unwrap_err();

        assert_eq!(engine.process(dispute(2, 3)), client_mismatch(2, 3, 1));
        assert_eq!(
            engine.process(dispute(1, 3)),
            Err(Rejection::UnknownTransaction(TxId(3)))
        );
    }

    #[test]
    fn a_dispute_from_an_unknown_client_opens_no_account() {
        let mut engine = Engine::default();
        let unknown = Err(Rejection::UnknownTransaction(TxId(1)));

        assert_eq!(engine.process(dispute(7, 1)), unknown);
        assert_eq!(engine.process(resolve(7, 1)), unknown);
        assert_eq!(engine.process(chargeback(7, 1)), unknown);
        assert_eq!(engine.accounts().count(), 0);
    }

    #[test]
    fn a_chargeback_freezes_only_that_client() {
        let mut engine = Engine::default();
        engine.process(deposit(1, 1, "3.0")).unwrap();
        engine.process(deposit(2, 2, "1.0")).unwrap();
        engine.process(dispute(1, 1)).unwrap();
        engine.process(chargeback(1, 1)).unwrap();

        engine.process(withdrawal(2, 3, "0.5")).unwrap();

        assert_eq!(account(&engine, 2).available(), Funds(5000));
        assert_eq!(
            engine.process(withdrawal(1, 4, "0.5")),
            Err(Rejection::AccountFrozen)
        );
    }

    #[test]
    fn a_withdrawal_refused_by_a_frozen_account_still_uses_up_its_transaction_id() {
        let mut engine = Engine::default();
        engine.process(deposit(1, 1, "3.0")).unwrap();
        engine.process(dispute(1, 1)).unwrap();
        engine.process(chargeback(1, 1)).unwrap();
        engine.process(withdrawal(1, 2, "1.0")).unwrap_err();

        assert_eq!(
            engine.process(deposit(1, 2, "1.0")),
            Err(Rejection::DuplicateTransaction(TxId(2)))
        );
        assert_eq!(account(&engine, 1).total(), Funds::ZERO);
    }
}
