# txn-engine

This document is for developers who operate or change txn-engine. It tells you how to use the program. It also tells you how the program calculates account values. It does not tell you how to install Rust.

The text of this document follows ASD-STE100 Simplified Technical English.

## Purpose

txn-engine is a command-line program. The program reads a CSV file that contains client transactions. Then the program calculates the end state of each client account. It writes the result as CSV to the standard output.

## Before you start

You must have these items:

- Cargo and Rust. This project uses cargo 1.97.1 and the 2024 edition of Rust.
- A C compiler and `make`. Cargo compiles the jemalloc allocator from C source.
- Python 3. You must have Python only to make the test fixtures again, or to measure the performance.

## Calculate the account values

To calculate the account values, do these steps:

1. Write the transactions to a CSV file. Refer to [Input format](#input-format).
2. Use this command:

   ```sh
   cargo run -- transactions.csv > accounts.csv
   ```

   Replace `transactions.csv` with the name of your input file.
3. Make sure that the exit code is 0. Refer to [Exit codes](#exit-codes).
4. Read the end state in `accounts.csv`. Refer to [Output format](#output-format).
5. Read the messages on the standard error. Refer to [Messages](#messages).

This is an example of an input file:

```csv
type, client, tx, amount
deposit, 1, 101, 50.0
deposit, 2, 102, 20.25
withdrawal, 1, 103, 12.5
deposit, 1, 104, 10.0
withdrawal, 2, 105, 30.0
dispute, 1, 104,
```

The program writes this result:

```csv
client,available,held,total,locked
1,37.5000,10.0000,47.5000,false
2,20.2500,0.0000,20.2500,false
```

The program also writes this message to the standard error:

```text
rejected Withdrawal { client: ClientId(2), tx: TxId(105), amount: Amount(300000) }: insufficient available funds
```

Client 2 has only 20.25 and tries to withdraw 30.0. The program rejects this withdrawal. It does not change the funds of client 2.

The dispute on transaction 104 moves 10.0 from `available` to `held`. The `total` of client 1 does not change.

## Exit codes

The program gives these exit codes:

| Exit code | Condition | Standard output |
|---|---|---|
| 0 | The program read all rows. Some rows can have a message. | All accounts. |
| 1 | The command has no input file. | Empty. |
| 1 | The program cannot open or read the input file. | Empty. |
| 1 | The header does not contain all four column names. | Empty. |
| 1 | The program cannot write to the standard error. | Empty. |
| 1 | The program cannot write to the standard output. | Can be incomplete. |

## Input format

The first line of the file must be the header. The header must contain the columns `type`, `client`, `tx`, and `amount`. The columns can be in any order.

The columns have these values:

| Column | Value | Limits |
|---|---|---|
| `type` | The name of the event. | `deposit`, `withdrawal`, `dispute`, `resolve`, or `chargeback`, in lowercase. |
| `client` | The client ID. | 0 to 65535. |
| `tx` | The transaction ID. | 0 to 4294967295. |
| `amount` | The quantity of money. | More than 0 and not more than 922337203685477.5807. |

Each row must obey these rules:

- A `deposit` or `withdrawal` row must have an amount.
- A `dispute`, `resolve`, or `chargeback` row must not have an amount.
- A row must not have more fields than the header.
- The rows `dispute,1,1,` and `dispute,1,1` are both correct.

An amount must have digits. It can also have a decimal point and 1 to 4 digits after the point. The program does not round an amount.

These amounts are correct: `5`, `5.0`, `5.1234`, and `0.0001`.

These amounts are not correct: `.5`, `1.`, `1e3`, `+1.0`, and `1.23456`.

The program removes ASCII spaces, tabs, and line breaks from the two ends of each field. It does not remove other space characters. The program does not count blank lines.

Do not put quotes around fields. The program does not use CSV quotes. A quote character in a row causes a message for that row only.

## Output format

The program writes one row for each account, in the sequence of the client IDs. Each amount has 4 digits after the decimal point.

The output has these columns:

| Column | Value |
|---|---|
| `client` | The client ID. |
| `available` | The funds that the client can withdraw. This value can be less than zero. |
| `held` | The funds that have an open dispute. |
| `total` | The sum of `available` and `held`. |
| `locked` | `true` if the account is frozen. Otherwise `false`. A chargeback makes an account frozen. |

## Account rules

### Funds changes

An event changes the funds of an account by the amount X, as this table shows:

| Event | `available` | `held` | `total` |
|---|---|---|---|
| Deposit | +X | 0 | +X |
| Withdrawal | -X | 0 | -X |
| Dispute of a deposit | -X | +X | 0 |
| Resolve of a deposit | +X | -X | 0 |
| Chargeback of a deposit | 0 | -X | -X |
| Dispute of a withdrawal | 0 | +X | +X |
| Resolve of a withdrawal | 0 | -X | -X |
| Chargeback of a withdrawal | +X | -X | 0 |

The money from a withdrawal is not in the account. For this reason, a dispute of a withdrawal adds a temporary credit to `held`. The client cannot withdraw this credit. A resolve removes the credit. A chargeback moves the credit to `available`, and the client gets the money again.

### Dispute rules

These rules apply to disputes:

- A withdrawal must not be more than `available`. The client cannot withdraw `held` funds.
- A dispute can make `available` less than zero. This occurs when the client spent the deposit before the dispute.
- A rejected withdrawal cannot have a dispute.
- After a resolve, the transaction can have a new dispute.
- After a chargeback, the transaction cannot have more events.

### Frozen accounts

A chargeback makes the account frozen, and the account stays frozen. A frozen account rejects withdrawals only. It accepts all other events, so that a client can pay back a negative `available` value.

### Transaction IDs and ownership

These rules apply to transaction IDs:

- Each deposit and withdrawal must have a new transaction ID.
- The program records the ID and the client when it first gets the ID. It also keeps this record for a rejected transaction.
- A row that is not correct does not use an ID.
- Only the client that first used an ID can send events for that ID.
- A deposit or withdrawal opens the account of the client, also if the program rejects it.
- A dispute, resolve, or chargeback does not open an account.

## Messages

The program writes messages to the standard error. Each message that starts with `skipped row` is for a row that is not correct. The program does not use this row. The run continues.

These messages tell you the cause:

| Message contains | Cause |
|---|---|
| `CSV deserialize error` | A field is not correct. |
| `more fields than the header` | The row has too many fields. |
| `has no amount` | A deposit or withdrawal has no amount. |
| `must not carry an amount` | A dispute, resolve, or chargeback has an amount. |
| `invalid amount` | The amount is not correct. Refer to [Input format](#input-format). |

Each message that starts with `rejected` is for a correct row that the rules do not permit. The account does not change. The run continues.

These messages tell you the cause:

| Message contains | `Rejection` value | Cause |
|---|---|---|
| `was already used` | `DuplicateTransaction` | The client used this ID before. |
| `expected client` | `ClientMismatch` | The ID belongs to a different client. |
| `insufficient available funds` | `InsufficientFunds` | The withdrawal is more than `available`. |
| `is unknown to this client` | `UnknownTransaction` | The client has no accepted transaction with this ID. |
| `is already disputed` | `AlreadyDisputed` | The transaction has an open dispute. |
| `is not disputed` | `NotDisputed` | The transaction has no open dispute. |
| `was already charged back` | `AlreadyChargedBack` | The transaction had a chargeback. |
| `account is frozen` | `AccountFrozen` | A frozen account cannot withdraw. |
| `balance would exceed` | `BalanceOverflow` | The result is too large for the account. |

A `ClientMismatch` message does not prove that the client in the row did a wrong action. The first client that uses an ID owns the ID, and a different client can use the ID first.

## Design

### Modules

The code has these modules:

| Module | Contents |
|---|---|
| `src/domain/account.rs` | `Account`, the aggregate root. It keeps the funds, the frozen state, and the transactions. |
| `src/domain/transaction.rs` | `Transaction`. It keeps the dispute status and gives the funds change for each event. |
| `src/domain/values.rs` | The value objects `ClientId`, `TxId`, `Amount`, and `Funds`. |
| `src/domain/event.rs` | `TransactionEvent`, with one variant for each event. |
| `src/domain/rejection.rs` | `Rejection`, with one variant for each rule. |
| `src/engine.rs` | `Engine`. It sends each event to the correct account and records the owner of each ID. |
| `src/csv_io.rs` | The anti-corruption layer. It changes CSV rows to domain events, and accounts to CSV rows. |
| `src/main.rs` | The command-line layer. It reads the arguments, writes the messages, and sets the exit code. |

### Design decisions

The design has these decisions:

- **Money.** `Amount` and `Funds` keep whole units of 0.0001 in an `i64`. The program does not use floating-point numbers, and it does not round.
- **Overflow.** The program checks all additions. If a result is too large, the program rejects the event and the account does not change.
- **Rejected events.** `Account` calculates all new values before it changes a value. Thus a rejected event cannot change a part of an account.
- **Event matches.** All matches on events and on dispute status are exhaustive. They have no wildcard arm. The compiler stops the build if a new variant has no arm.
- **Errors.** `Rejection`, `RowError`, and `InvalidAmount` use `thiserror` at the module boundaries. `src/main.rs` uses `anyhow`.
- **Input.** `read_events` reads through one `ByteRecord` and deserializes into a `Row` that borrows its fields. A row does not cause a heap allocation.
- **Output.** `write_accounts` writes all fields through one `String` buffer. An account row does not cause a heap allocation.
- **Field trim.** The program does not use `Trim::All`, because `Trim::All` makes a new record for each row. The program trims each field when it deserializes the field.
- **Allocator.** The binary uses jemalloc. With the `dhat-heap` feature, the binary uses the DHAT allocator, because a binary can have only one global allocator.
- **Messages.** The program writes messages through a `BufWriter`. Messages arrive in blocks, and the program flushes them before it writes the accounts.

## Tests

To do all tests, use this command:

```sh
cargo test
```

The tests have these levels:

| Level | Location | Function |
|---|---|---|
| `Account` API | `src/domain/` | Tests the money rules and the dispute status. |
| `Engine::process` | `src/engine.rs` | Tests ID uniqueness, ownership, and account selection. |
| The binary | `tests/cli.rs` | Tests the CSV formats, the messages, and the exit codes. |
| Fixtures | `tests/fixtures.rs` | Compares the binary with an independent ledger. |

### Fixtures

The fixtures are in `tests/fixtures/`. Each fixture has these files:

- `transactions.csv` is the input.
- `accounts.csv` is the expected output.
- `outcomes.csv` is the expected count of each type of message.

The fixtures have these sizes:

| Fixture | Clients | Events |
|---|---|---|
| `tiny` | 3 | 61 |
| `small` | 25 | 1,000 |
| `medium` | 1,000 | 25,000 |
| `large` | 10,000 | 100,000 |
| `max` | 65,536 | 500,000 |

The file `tests/fixtures/generate.py` contains a ledger in Python. This ledger comes from the account rules and not from the Rust code. The `tiny` fixture also has results that a person calculated. The generator compares its ledger with these results.

Each fixture contains events from all rule categories, and rows that are not correct. The counts of the events do not include blank lines.

To make the fixtures again, do these steps:

1. Use this command:

   ```sh
   python3 tests/fixtures/generate.py
   ```

2. Use the `cargo test` command.

The generator gives the same bytes each time.

## Performance

This table gives reference values for the fixtures:

| Fixture | Clients | Events | Median | Fastest | Slowest | Events/s | Peak memory |
|---|---:|---:|---:|---:|---:|---:|---:|
| `tiny` | 3 | 61 | 4.35 ms | 3.83 ms | 5.17 ms | 14,024 | 2.7 MiB |
| `small` | 25 | 1,000 | 4.51 ms | 4.15 ms | 5.65 ms | 221,735 | 2.8 MiB |
| `medium` | 1,000 | 25,000 | 13.23 ms | 12.24 ms | 14.75 ms | 1,889,276 | 3.8 MiB |
| `large` | 10,000 | 100,000 | 41.45 ms | 39.28 ms | 45.11 ms | 2,412,355 | 8.2 MiB |
| `max` | 65,536 | 500,000 | 226.34 ms | 207.84 ms | 272.87 ms | 2,209,094 | 28.0 MiB |

The values come from this configuration:

- The computer had an ARM64 CPU and 64 GiB of memory, with macOS.
- The binary was a release build from rustc 1.97.1, with jemalloc.
- Each row shows 20 runs, after one run that the script does not count.
- The standard output and the standard error went to files.
- The computer did other work during the runs. The load average was approximately 6.0.

The time for each run includes the start of the process, which is approximately 4 ms. Thus the Events/s values for `tiny` and `small` are low. Use the `medium`, `large`, and `max` rows to compare throughput.

These values are not limits. Compare only the values from the same computer.

To measure the performance again, do these steps:

1. Use this command:

   ```sh
   cargo build --release
   ```

2. Use this command:

   ```sh
   python3 tests/fixtures/measure.py
   ```

The script writes a table in the format that this section uses.

## Profile the program

Use the profiles to find the code that uses the most heap memory or CPU time. Do not use a profile to measure the time of a run. DHAT makes the program approximately 40 times slower.

To find the heap use, do these steps:

1. Use this command:

   ```sh
   cargo run --profile profiling --features dhat-heap -- tests/fixtures/max/transactions.csv > /dev/null 2> /dev/null
   ```

2. Open `dhat-heap.json` in the [DHAT viewer](https://nnethercote.github.io/dh_view/dh_view.html).

The `profiling` build profile is a release build with line tables. Thus the viewer shows file names and line numbers. Git does not track `dhat-heap.json`.

To find the CPU use on macOS, do these steps:

1. Use this command:

   ```sh
   cargo build --profile profiling
   ```

2. Use these commands with an input file that has more than 1,000,000 events:

   ```sh
   target/profiling/txn-engine TRANSACTIONS_FILE > /dev/null 2> /dev/null &
   sample $! 15 1 -mayDie -file sample.txt
   ```

   Replace `TRANSACTIONS_FILE` with the name of the input file.
3. Read the section "Sort by top of stack" in `sample.txt`.

## Limits

**CAUTION:** Do not use an input file from a source that you do not trust. A file with no line breaks stays in memory and can use all the memory of the computer.

The program has these limits:

- The memory use increases with the number of transactions. The engine must keep each transaction for a possible dispute.
- The program accepts a plus sign in an ID. For example, it reads `+1` as the client 1.
- If the shell closes the standard output, the program can stop with exit code 0 and write no accounts. For example, this occurs with `>&-`.
- The program keeps messages in a buffer during the run. If you stop the program with a signal, you can lose these messages.
