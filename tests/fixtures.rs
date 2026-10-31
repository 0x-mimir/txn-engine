//! Compares the binary with the independent ledger in `tests/fixtures/generate.py`.

use std::fs;
use std::path::Path;
use std::process::Command;

fn check(tier: &str) {
    let directory = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/fixtures")
        .join(tier);
    let output = Command::new(env!("CARGO_BIN_EXE_txn-engine"))
        .arg(directory.join("transactions.csv"))
        .output()
        .unwrap();
    assert!(output.status.success(), "{tier} failed");

    let expected = fs::read_to_string(directory.join("accounts.csv")).unwrap();
    let accounts = String::from_utf8(output.stdout).unwrap();
    for (number, (actual, expected)) in accounts.lines().zip(expected.lines()).enumerate() {
        assert_eq!(actual, expected, "{tier} accounts line {}", number + 1);
    }
    assert_eq!(accounts.lines().count(), expected.lines().count());

    let log = String::from_utf8_lossy(&output.stderr);
    let outcomes = fs::read_to_string(directory.join("outcomes.csv")).unwrap();
    let mut rejected = 0;
    for line in outcomes.lines() {
        let (outcome, count) = line.rsplit_once(',').unwrap();
        let count: usize = count.parse().unwrap();
        assert_eq!(log.matches(outcome).count(), count, "{tier}: {outcome}");
        rejected += if outcome == "skipped row" { 0 } else { count };
    }
    assert_eq!(log.matches("rejected ").count(), rejected, "{tier}");
}

#[test]
fn tiny_matches_the_hand_checked_scenario() {
    check("tiny");
}

#[test]
fn small_matches_the_independent_ledger() {
    check("small");
}

#[test]
fn medium_matches_the_independent_ledger() {
    check("medium");
}

#[test]
fn large_matches_the_independent_ledger() {
    check("large");
}

#[test]
fn max_clients_and_500_000_events_match_the_independent_ledger() {
    check("max");
}
