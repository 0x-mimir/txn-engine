use std::path::PathBuf;
use std::process::{Command, Output, Stdio};
use std::{fs, io, thread};

fn transactions_file(csv: &str) -> PathBuf {
    let name = thread::current().name().unwrap().replace("::", "_");
    let path = PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join(format!("{name}.csv"));
    fs::write(&path, csv).unwrap();
    path
}

fn run(csv: &str) -> Output {
    Command::new(env!("CARGO_BIN_EXE_txn-engine"))
        .arg(transactions_file(csv))
        .output()
        .unwrap()
}

fn stdout(output: &Output) -> String {
    String::from_utf8(output.stdout.clone()).unwrap()
}

fn stderr(output: &Output) -> String {
    String::from_utf8(output.stderr.clone()).unwrap()
}

#[test]
fn prints_the_end_state_of_each_account() {
    let output = run("type, client, tx, amount
deposit, 1, 101, 50.0
deposit, 2, 102, 20.25
withdrawal, 1, 103, 12.5
deposit, 1, 104, 10.0
withdrawal, 2, 105, 30.0
dispute, 1, 104,
");

    assert!(output.status.success());
    assert_eq!(
        stdout(&output),
        "client,available,held,total,frozen
1,37.5000,10.0000,47.5000,false
2,20.2500,0.0000,20.2500,false
"
    );
}

#[test]
fn accepts_reference_rows_with_or_without_the_trailing_comma() {
    let output = run("type,client,tx,amount
deposit,1,1,3.0
deposit,1,2,1.0
dispute,1,1,
resolve,1,1
dispute,1,2
");

    assert_eq!(
        stdout(&output),
        "client,available,held,total,frozen
1,3.0000,1.0000,4.0000,false
"
    );
}

#[test]
fn trims_spaces_around_every_field() {
    let output = run("type , client , tx , amount
 deposit , 1 , 1 , 3.0
  deposit  ,  1  ,  2  ,  1.0  
 dispute , 1 , 1 , 
 resolve , 1 , 1
 dispute , 1 , 2 ,
");

    assert_eq!(
        stdout(&output),
        "client,available,held,total,frozen
1,3.0000,1.0000,4.0000,false
"
    );
}

#[test]
fn accepts_windows_line_endings() {
    let output = run("type,client,tx,amount\r\ndeposit,1,1,3.0\r\ndispute,1,1,\r\n");

    assert_eq!(
        stdout(&output),
        "client,available,held,total,frozen
1,0.0000,3.0000,3.0000,false
"
    );
}

#[test]
fn a_chargeback_is_reported_as_frozen_with_negative_available() {
    let output = run("type,client,tx,amount
deposit,1,1,3.0
withdrawal,1,2,3.0
dispute,1,1,
chargeback,1,1,
");

    assert_eq!(
        stdout(&output),
        "client,available,held,total,frozen
1,-3.0000,0.0000,-3.0000,true
"
    );
}

#[test]
fn a_disputed_withdrawal_is_reported_as_a_held_credit() {
    let output = run("type,client,tx,amount
deposit,1,1,10.0
withdrawal,1,2,3.0
dispute,1,2,
");

    assert_eq!(
        stdout(&output),
        "client,available,held,total,frozen
1,7.0000,3.0000,10.0000,false
"
    );
}

#[test]
fn clients_are_listed_in_id_order_regardless_of_input_order() {
    let output = run("type,client,tx,amount
deposit,9,1,1.0
deposit,2,2,1.0
");

    assert_eq!(
        stdout(&output),
        "client,available,held,total,frozen
2,1.0000,0.0000,1.0000,false
9,1.0000,0.0000,1.0000,false
"
    );
}

#[test]
fn malformed_rows_are_reported_and_skipped_without_stopping_the_run() {
    let output = run("type,client,tx,amount
deposit,1,1,1.0
transfer,1,2,1.0
deposit,70000,3,1.0
deposit,1,5000000000,1.0
deposit,1,4,
deposit,1,5
deposit,1,6,1.23456
deposit,1,7,-1.0
deposit,1,8,abc
deposit,1,10,   
deposit,1,9,2.0
");

    assert!(output.status.success());
    assert_eq!(
        stdout(&output),
        "client,available,held,total,frozen
1,3.0000,0.0000,3.0000,false
"
    );
    assert_eq!(stderr(&output).matches("skipped row").count(), 9);
}

#[test]
fn rejections_go_to_stderr_and_leave_stdout_as_clean_csv() {
    let output = run("type,client,tx,amount
deposit,1,1,1.0
withdrawal,1,2,5.0
");

    assert!(output.status.success());
    assert_eq!(
        stdout(&output),
        "client,available,held,total,frozen
1,1.0000,0.0000,1.0000,false
"
    );
    assert!(stderr(&output).contains("insufficient available funds"));
}

#[test]
fn every_rejection_is_reported_when_they_outgrow_the_stderr_buffer() {
    let refused = "withdrawal,1,2,5.0\n".repeat(1000);
    let output = run(&format!("type,client,tx,amount\n{refused}"));

    assert!(output.status.success());
    assert_eq!(stderr(&output).matches("rejected").count(), 1000);
}

#[test]
fn an_event_from_the_wrong_client_is_reported_with_both_clients() {
    let output = run("type,client,tx,amount
deposit,1,1,3.0
deposit,2,2,1.0
dispute,2,1,
");

    assert_eq!(
        stdout(&output),
        "client,available,held,total,frozen
1,3.0000,0.0000,3.0000,false
2,1.0000,0.0000,1.0000,false
"
    );
    assert!(stderr(&output).contains("transaction 1 expected client 1 but got client 2"));
}

#[test]
fn a_file_with_no_transactions_prints_only_the_header() {
    let output = run("type,client,tx,amount\n");

    assert!(output.status.success());
    assert_eq!(stdout(&output), "client,available,held,total,frozen\n");
}

#[test]
fn a_missing_file_fails_without_printing_accounts() {
    let output = Command::new(env!("CARGO_BIN_EXE_txn-engine"))
        .arg("no-such-file.csv")
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(stdout(&output), "");
    assert!(stderr(&output).contains("no-such-file.csv"));
}

#[test]
fn running_without_a_path_prints_the_usage() {
    let output = Command::new(env!("CARGO_BIN_EXE_txn-engine"))
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(stdout(&output), "");
    assert!(stderr(&output).contains("usage: txn-engine"));
}

#[test]
fn a_stray_quote_loses_only_its_own_row() {
    let output = run("type,client,tx,amount
deposit,1,1,10.0
deposit,1,2,\"5.0
dispute,1,1,\"
deposit,1,3,500.0
deposit,2,4,700.0
");

    assert!(output.status.success());
    assert_eq!(
        stdout(&output),
        "client,available,held,total,frozen
1,510.0000,0.0000,510.0000,false
2,700.0000,0.0000,700.0000,false
"
    );
    assert_eq!(stderr(&output).matches("skipped row").count(), 2);
}

#[test]
fn a_row_with_more_fields_than_the_header_is_skipped() {
    let output = run("type,client,tx,amount
deposit,1,1,1,000.50
deposit,1,2,1.0,extra
deposit,1,3,2.0
dispute,1,3,,junk
");

    assert_eq!(
        stdout(&output),
        "client,available,held,total,frozen
1,2.0000,0.0000,2.0000,false
"
    );
    assert_eq!(stderr(&output).matches("skipped row").count(), 3);
}

#[test]
fn an_amount_on_a_reference_row_is_skipped() {
    let output = run("type,client,tx,amount
deposit,1,1,10.0
dispute,1,1,5.0
resolve,1,1,-9.99
chargeback,1,1,abc
");

    assert_eq!(
        stdout(&output),
        "client,available,held,total,frozen
1,10.0000,0.0000,10.0000,false
"
    );
    assert_eq!(stderr(&output).matches("skipped row").count(), 3);
}

#[test]
fn columns_can_come_in_any_order() {
    let output = run("amount,tx,client,type
1.5,1,2,deposit
");

    assert_eq!(
        stdout(&output),
        "client,available,held,total,frozen
2,1.5000,0.0000,1.5000,false
"
    );
}

#[test]
fn a_file_without_the_required_columns_fails_without_printing_accounts() {
    for csv in [
        "deposit,1,1,1.0\n",
        "typ,client,tx,amount\n",
        "type,client,tx\n",
        "",
    ] {
        let output = run(csv);

        assert!(!output.status.success(), "accepted {csv:?}");
        assert_eq!(stdout(&output), "");
        assert!(stderr(&output).contains("column"), "for {csv:?}");
    }
}

#[test]
fn only_ascii_whitespace_is_trimmed_around_fields() {
    let output = run("type,client,tx,amount
deposit,1,1,\u{a0}1.0
deposit,\u{3000}1,2,1.0
deposit,1,3,2.0
");

    assert_eq!(
        stdout(&output),
        "client,available,held,total,frozen
1,2.0000,0.0000,2.0000,false
"
    );
    assert_eq!(stderr(&output).matches("skipped row").count(), 2);
}

#[test]
fn a_directory_path_fails_without_printing_accounts() {
    let output = Command::new(env!("CARGO_BIN_EXE_txn-engine"))
        .arg(env!("CARGO_TARGET_TMPDIR"))
        .output()
        .unwrap();

    assert!(!output.status.success());
    assert_eq!(stdout(&output), "");
}

#[cfg(unix)]
#[test]
fn a_path_that_is_not_utf8_fails_cleanly() {
    use std::ffi::OsStr;
    use std::os::unix::ffi::OsStrExt;

    let output = Command::new(env!("CARGO_BIN_EXE_txn-engine"))
        .arg(OsStr::from_bytes(b"no-such-\xff.csv"))
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
}

#[test]
fn a_closed_stdout_fails_the_run() {
    let (reader, writer) = io::pipe().unwrap();
    drop(reader);

    let status = Command::new(env!("CARGO_BIN_EXE_txn-engine"))
        .arg(transactions_file(
            "type,client,tx,amount\ndeposit,1,1,1.0\n",
        ))
        .stdout(writer)
        .stderr(Stdio::null())
        .status()
        .unwrap();

    assert_eq!(status.code(), Some(1));
}

#[test]
fn a_closed_stderr_fails_the_run_instead_of_losing_rejections() {
    let (reader, writer) = io::pipe().unwrap();
    drop(reader);

    let output = Command::new(env!("CARGO_BIN_EXE_txn-engine"))
        .arg(transactions_file("type,client,tx,amount\nbad,1,1,1.0\n"))
        .stderr(writer)
        .output()
        .unwrap();

    assert_eq!(output.status.code(), Some(1));
    assert_eq!(stdout(&output), "");
}
