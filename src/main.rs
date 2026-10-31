use std::env;
use std::fs::File;
use std::io::{self, BufWriter, Write};
use std::path::PathBuf;

use anyhow::{Context, Result};
use txn_engine::csv_io::{RowError, read_events, write_accounts};
use txn_engine::engine::Engine;

const LOG_FAILURE: &str = "cannot write to stderr";

fn main() -> Result<()> {
    let path = env::args_os()
        .nth(1)
        .map(PathBuf::from)
        .context("usage: txn-engine <transactions.csv>")?;
    let path = path.as_path();
    let file = File::open(path).with_context(|| format!("cannot open {}", path.display()))?;

    let mut log = BufWriter::new(io::stderr().lock());
    let mut engine = Engine::default();
    let events = read_events(file).with_context(|| format!("cannot read {}", path.display()))?;
    for event in events {
        let event = match event {
            Ok(event) => event,
            // Accounts built from part of the input would look complete, so stop instead.
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

    log.flush().context(LOG_FAILURE)?;
    write_accounts(engine.accounts(), io::stdout().lock()).context("cannot write accounts")
}
