//! txn-engine reads client transactions from a CSV file. It calculates the end state of each
//! client account.
//!
//! - [`domain`] contains the account rules.
//! - [`engine`] sends each event to the correct account.
//! - [`csv_io`] changes CSV rows to events, and accounts to CSV rows.

pub mod csv_io;
pub mod domain;
pub mod engine;
