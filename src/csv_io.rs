//! The anti-corruption layer. It changes CSV rows to domain events, and accounts to CSV rows.

use std::fmt::{Display, Write as _};
use std::io::{Read, Write};
use std::iter;
use std::str::FromStr;

use csv::{ByteRecord, ReaderBuilder, Trim};
use serde::de::{self, IntoDeserializer};
use serde::{Deserialize, Deserializer};

use crate::domain::{Account, ClientId, InvalidAmount, TransactionEvent, TxId};

/// The columns that the header of the input file must contain.
const REQUIRED_COLUMNS: [&str; 4] = ["type", "client", "tx", "amount"];

/// The header of the output.
const ACCOUNT_COLUMNS: [&str; 5] = ["client", "available", "held", "total", "locked"];

/// The value of the `type` column.
#[derive(Debug, Clone, Copy, Deserialize)]
#[serde(rename_all = "lowercase")]
enum RowType {
    Deposit,
    Withdrawal,
    Dispute,
    Resolve,
    Chargeback,
}

/// Uses the serde names of the variants, so that the names stay in one location.
impl FromStr for RowType {
    type Err = de::value::Error;

    fn from_str(value: &str) -> Result<Self, Self::Err> {
        Self::deserialize(value.into_deserializer())
    }
}

/// One row of the input. This type borrows its fields from the reused record, so a row does not
/// cause a heap allocation.
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

/// Trims a field and parses it. Do not use `Trim::All`, because it makes a new record for each row.
fn trimmed<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: Deserializer<'de>,
    T: FromStr,
    T::Err: Display,
{
    let field = <&str>::deserialize(deserializer)?;
    field.trim_ascii().parse().map_err(de::Error::custom)
}

/// The reason for which the program cannot change the input to events.
#[derive(Debug, thiserror::Error)]
pub enum RowError {
    /// The reader failed, or a field is not correct.
    #[error(transparent)]
    Csv(#[from] csv::Error),
    /// The header does not contain this column.
    #[error("the header has no `{0}` column")]
    MissingColumn(&'static str),
    /// The row has more fields than the header. A comma in an amount can cause this error.
    #[error("line {line} has more fields than the header")]
    TooManyFields {
        /// The line number in the input file.
        line: u64,
    },
    /// A deposit or withdrawal has no amount.
    #[error("transaction {tx} has no amount")]
    MissingAmount {
        /// The transaction ID in the row.
        tx: u32,
    },
    /// A dispute, resolve, or chargeback has an amount.
    #[error("transaction {tx} must not carry an amount")]
    UnexpectedAmount {
        /// The transaction ID in the row.
        tx: u32,
    },
    /// The amount is not correct.
    #[error(transparent)]
    InvalidAmount(#[from] InvalidAmount),
}

/// Changes the types of the CSV format to domain types. All row errors are found here.
impl TryFrom<Row<'_>> for TransactionEvent {
    type Error = RowError;

    fn try_from(row: Row<'_>) -> Result<Self, RowError> {
        let client = ClientId(row.client);
        let tx = TxId(row.tx);
        // The value `dispute, 1, 1, ` has an amount field that contains only a space.
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

/// Reads events from `input`, one row at a time.
///
/// All rows use one reused record, so the memory use does not increase with the number of rows.
/// Each item is one event, or the error for one row.
///
/// # Errors
///
/// Gives an error if the program cannot read the header, or if the header has no required column.
// ponytail: A full line stays in memory, because csv has no limit on the record size. Add a limit
// if the input can come from an unknown source.
pub fn read_events(
    input: impl Read,
) -> Result<impl Iterator<Item = Result<TransactionEvent, RowError>>, RowError> {
    let mut reader = ReaderBuilder::new()
        .trim(Trim::Headers)
        .flexible(true) // The row `dispute,1,1` has no amount field.
        .quoting(false) // With quotes, one stray `"` hides the rows that follow it.
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

/// Changes one record to an event.
fn parse_event(record: &ByteRecord, headers: &ByteRecord) -> Result<TransactionEvent, RowError> {
    if record.len() > headers.len() {
        // Serde does not see the extra fields, so check them here.
        let line = record.position().map_or(0, csv::Position::line);
        return Err(RowError::TooManyFields { line });
    }
    record.deserialize::<Row<'_>>(Some(headers))?.try_into()
}

/// Writes `accounts` to `output` as CSV, with 4 digits after each decimal point.
///
/// # Errors
///
/// Gives an error if a write to `output` fails.
pub fn write_accounts<'a>(
    accounts: impl Iterator<Item = &'a Account>,
    output: impl Write,
) -> csv::Result<()> {
    let mut writer = csv::Writer::from_writer(output);
    writer.write_record(ACCOUNT_COLUMNS)?;
    let mut buffer = String::new();
    for account in accounts {
        write_field(&mut writer, &mut buffer, account.client().0)?;
        write_field(&mut writer, &mut buffer, account.available())?;
        write_field(&mut writer, &mut buffer, account.held())?;
        write_field(&mut writer, &mut buffer, account.total())?;
        write_field(&mut writer, &mut buffer, account.is_frozen())?;
        // An empty record ends the row of fields.
        writer.write_record(None::<&[u8]>)?;
    }
    // Flush here, because the drop of the writer does not show a write error.
    writer.flush()?;
    Ok(())
}

/// Writes one field through `buffer`, so that a field does not cause a heap allocation.
fn write_field(
    writer: &mut csv::Writer<impl Write>,
    buffer: &mut String,
    value: impl Display,
) -> csv::Result<()> {
    buffer.clear();
    write!(buffer, "{value}").expect("a write to a String cannot fail");
    writer.write_field(&*buffer)
}
