//! Anti-corruption layer: the CSV formats on one side, domain types on the other.

use std::fmt::Display;
use std::io::{Read, Write};
use std::iter;
use std::str::FromStr;

use csv::{ByteRecord, ReaderBuilder, Trim};
use serde::de::{self, IntoDeserializer};
use serde::{Deserialize, Deserializer};

use crate::domain::{Account, ClientId, InvalidAmount, TransactionEvent, TxId};

const REQUIRED_COLUMNS: [&str; 4] = ["type", "client", "tx", "amount"];

#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum RowType {
    Deposit,
    Withdrawal,
    Dispute,
    Resolve,
    Chargeback,
}

impl FromStr for RowType {
    type Err = de::value::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::deserialize(value.into_deserializer())
    }
}

/// Borrows its fields from the reused record, so a row allocates nothing.
#[derive(Debug, Deserialize)]
struct Row<'a> {
    #[serde(rename = "type", deserialize_with = "trimmed")]
    row_type: RowType,
    #[serde(deserialize_with = "trimmed")]
    client: u16,
    #[serde(deserialize_with = "trimmed")]
    tx: u32,
    #[serde(default, borrow)]
    amount: Option<&'a str>,
}

/// Stands in for `Trim::All`, which rebuilds the record and so allocates on every row.
fn trimmed<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr,
    T::Err: Display,
{
    let field = <&str>::deserialize(deserializer)?;
    field.trim_ascii().parse().map_err(de::Error::custom)
}

#[derive(Debug, thiserror::Error)]
pub enum RowError {
    #[error(transparent)]
    Csv(#[from] csv::Error),
    #[error("the header has no `{0}` column")]
    MissingColumn(&'static str),
    #[error("line {line} has more fields than the header")]
    TooManyFields { line: u64 },
    #[error("transaction {tx} has no amount")]
    MissingAmount { tx: u32 },
    #[error("transaction {tx} must not carry an amount")]
    UnexpectedAmount { tx: u32 },
    #[error(transparent)]
    InvalidAmount(#[from] InvalidAmount),
}

impl TryFrom<Row<'_>> for TransactionEvent {
    type Error = RowError;

    fn try_from(row: Row<'_>) -> Result<Self, RowError> {
        let client = ClientId(row.client);
        let tx = TxId(row.tx);
        let amount = row.amount.map(str::trim_ascii);
        Ok(
            match (row.row_type, amount.filter(|amount| !amount.is_empty())) {
                (RowType::Deposit, Some(amount)) => Self::Deposit {
                    client,
                    tx,
                    amount: amount.parse()?,
                },
                (RowType::Withdrawal, Some(amount)) => Self::Withdrawal {
                    client,
                    tx,
                    amount: amount.parse()?,
                },
                (RowType::Dispute, None) => Self::Dispute { client, tx },
                (RowType::Resolve, None) => Self::Resolve { client, tx },
                (RowType::Chargeback, None) => Self::Chargeback { client, tx },
                (RowType::Deposit | RowType::Withdrawal, None) => {
                    return Err(RowError::MissingAmount { tx: row.tx });
                }
                (RowType::Dispute | RowType::Resolve | RowType::Chargeback, Some(_)) => {
                    return Err(RowError::UnexpectedAmount { tx: row.tx });
                }
            },
        )
    }
}

/// Streams events through one reused record, so memory stays flat however many rows arrive.
// ponytail: a single line is still buffered whole, as csv has no record size limit. Cap the
// reader if inputs ever stop being trusted local files.
pub fn read_events(
    input: impl Read,
) -> Result<impl Iterator<Item = Result<TransactionEvent, RowError>>, RowError> {
    let mut reader = ReaderBuilder::new()
        .trim(Trim::Headers)
        .flexible(true) // `dispute,1,1` may omit the amount column altogether.
        .quoting(false) // Nothing here is quoted, and a stray `"` would hide the rows after it.
        .from_reader(input);
    let headers = reader.byte_headers()?.clone();
    let has_column = |column: &&str| headers.iter().any(|header| header == column.as_bytes());
    if let Some(column) = REQUIRED_COLUMNS.iter().find(|column| !has_column(column)) {
        return Err(RowError::MissingColumn(column));
    }
    let mut record = ByteRecord::new();
    Ok(iter::from_fn(move || {
        match reader.read_byte_record(&mut record) {
            Ok(true) => Some(parse_event(&record, &headers)),
            Ok(false) => None,
            Err(error) => Some(Err(error.into())),
        }
    }))
}

fn parse_event(record: &ByteRecord, headers: &ByteRecord) -> Result<TransactionEvent, RowError> {
    if record.len() > headers.len() {
        let line = record.position().map_or(0, csv::Position::line);
        return Err(RowError::TooManyFields { line });
    }
    record.deserialize::<Row<'_>>(Some(headers))?.try_into()
}

pub fn write_accounts<'a>(
    accounts: impl Iterator<Item = &'a Account>,
    output: impl Write,
) -> csv::Result<()> {
    let mut writer = csv::Writer::from_writer(output);
    writer.write_record(["client", "available", "held", "total", "frozen"])?;
    for account in accounts {
        writer.write_record([
            account.client().0.to_string(),
            account.available().to_string(),
            account.held().to_string(),
            account.total().to_string(),
            account.is_frozen().to_string(),
        ])?;
    }
    writer.flush()?;
    Ok(())
}
