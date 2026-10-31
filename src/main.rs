//! The command-line layer. It reads the arguments, writes the messages, and sets the exit code.

use std::env;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};
use txn_engine::csv_io::{RowError, read_events, write_accounts};
use txn_engine::engine::Engine;

/// The error if the program cannot write to the standard error.
const LOG_FAILURE: &str = "cannot write to stderr";

/// Reads the input file, and writes the accounts to the standard output.
///
/// A row that is not correct and a rejected event cause a message only. All other errors stop
/// the program, and the program then writes no accounts.
fn main() -> Result<()> {
    // Use `args_os`, because a file name is not always UTF-8.
    let path = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .context("usage: txn-engine <transactions.csv>")?;
    let path = path.as_path();
    let file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;

    // Keep the messages in a buffer, because one write for each message is slow.
    let mut log = BufWriter::new(io::stderr().lock());
    let mut engine = Engine::default();
    let events = read_events(file).with_context(|| format!("cannot read {}", path.display()))?;
    for event in events {
        let event = match event {
            Ok(event) => event,
            // Stop here, because accounts from a part of the input are not correct.
            Err(RowError::Csv(error)) if error.is_io_error() => {
                return Err(error).with_context(|| format!("cannot read {}", path.display()));
            }
            Err(error) => {
                writeln!(log, "skipped row: {error}").context(LOG_FAILURE)?;
                continue;
            }
        };
        if let Err(rejection) = engine.process(event) {
            writeln!(log, "rejected {event:?}: {rejection}").context(LOG_FAILURE)?;
        }
    }

    // Flush here, because a lost message must stop the run before the accounts go out.
    log.flush().context(LOG_FAILURE)?;
    write_accounts(engine.accounts(), io::stdout().lock()).context("cannot write accounts")
}
