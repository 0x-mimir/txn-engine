#!/usr/bin/env python3
"""Makes adversarial transaction fixtures and their expected results.

The expected results come from the Ledger class in this file. The Ledger class comes from
the account rules and not from the Rust code. Thus tests/fixtures.rs compares two
independent implementations. This script gives the same bytes each time.

Usage: python3 tests/fixtures/generate.py
"""

import random
import re
from collections import Counter
from pathlib import Path

I64_MAX = 2**63 - 1
I64_MIN = -(2**63)
MAX_AMOUNT = "922337203685477.5807"
ASCII_WHITESPACE = b" \t\n\x0c\r"
HEADER = b"type, client, tx, amount\n"

SKIPPED = "skipped row"
DUPLICATE = "was already used"
MISMATCH = "expected client"
INSUFFICIENT = "insufficient available funds"
UNKNOWN = "is unknown to this client"
ALREADY_DISPUTED = "is already disputed"
NOT_DISPUTED = "is not disputed"
ALREADY_CHARGED_BACK = "was already charged back"
FROZEN = "account is frozen"
OVERFLOW = "balance would exceed"
APPLIED = "applied"
REJECTIONS = (
    DUPLICATE,
    MISMATCH,
    INSUFFICIENT,
    UNKNOWN,
    ALREADY_DISPUTED,
    NOT_DISPUTED,
    ALREADY_CHARGED_BACK,
    FROZEN,
    OVERFLOW,
)


def parse_amount(text):
    """Gives the amount in whole units of 0.0001, or None if the amount is not correct."""
    if not re.fullmatch(r"[0-9]+(\.[0-9]{1,4})?", text):
        return None
    whole, _, fraction = text.partition(".")
    units = int(whole) * 10_000 + int(fraction.ljust(4, "0") or 0)
    return units if 0 < units <= I64_MAX else None


def parse_id(text, largest):
    if not re.fullmatch(r"\+?[0-9]+", text) or int(text) > largest:
        return None
    return int(text)


class Account:
    def __init__(self):
        self.available = 0
        self.held = 0
        self.frozen = False
        # tx -> [kind, amount, status]
        self.transactions = {}

    def move(self, available, held):
        """Changes the balances. Gives False and changes nothing if a balance is out of range."""
        new_available = self.available + available
        new_held = self.held + held
        balances = (new_available, new_held, new_available + new_held)
        if not all(I64_MIN <= balance <= I64_MAX for balance in balances):
            return False
        self.available, self.held = new_available, new_held
        return True


class Ledger:
    def __init__(self):
        self.owners = {}
        self.accounts = {}
        self.outcomes = Counter()

    def row(self, raw):
        outcome = self.apply(raw)
        self.outcomes[outcome] += 1
        return outcome

    def apply(self, raw):
        fields = raw.split(b",")
        if not 3 <= len(fields) <= 4:
            return SKIPPED
        try:
            text = [field.decode("utf-8") for field in fields]
        except UnicodeDecodeError:
            return SKIPPED
        trimmed = [
            field.encode("utf-8").strip(ASCII_WHITESPACE).decode("utf-8")
            for field in text
        ]
        kind = trimmed[0]
        client = parse_id(trimmed[1], 2**16 - 1)
        tx = parse_id(trimmed[2], 2**32 - 1)
        amount = trimmed[3] if len(trimmed) == 4 and trimmed[3] else None
        if client is None or tx is None:
            return SKIPPED
        if kind in ("deposit", "withdrawal"):
            units = parse_amount(amount) if amount is not None else None
            return SKIPPED if units is None else self.funds(kind, client, tx, units)
        if kind in ("dispute", "resolve", "chargeback"):
            return SKIPPED if amount is not None else self.reference(kind, client, tx)
        return SKIPPED

    def funds(self, kind, client, tx, units):
        if tx in self.owners:
            return DUPLICATE if self.owners[tx] == client else MISMATCH
        self.owners[tx] = client
        account = self.accounts.setdefault(client, Account())
        if kind == "deposit":
            moved = account.move(units, 0)
        else:
            if account.frozen:
                return FROZEN
            if units > account.available:
                return INSUFFICIENT
            moved = account.move(-units, 0)
        if not moved:
            return OVERFLOW
        account.transactions[tx] = [kind, units, "undisputed"]
        return APPLIED

    def reference(self, kind, client, tx):
        if tx not in self.owners:
            return UNKNOWN
        if self.owners[tx] != client:
            return MISMATCH
        account = self.accounts[client]
        transaction = account.transactions.get(tx)
        if transaction is None:
            return UNKNOWN
        source, units, status = transaction
        if status == "charged back":
            return ALREADY_CHARGED_BACK
        if kind == "dispute":
            if status == "disputed":
                return ALREADY_DISPUTED
            available = -units if source == "deposit" else 0
            moved, status = account.move(available, units), "disputed"
        elif status == "undisputed":
            return NOT_DISPUTED
        elif kind == "resolve":
            available = units if source == "deposit" else 0
            moved, status = account.move(available, -units), "undisputed"
        else:
            available = units if source == "withdrawal" else 0
            moved, status = account.move(available, -units), "charged back"
        if not moved:
            return OVERFLOW
        transaction[2] = status
        account.frozen = account.frozen or status == "charged back"
        return APPLIED

    def accounts_csv(self):
        lines = ["client,available,held,total,locked"]
        for client in sorted(self.accounts):
            account = self.accounts[client]
            total = account.available + account.held
            balances = map(funds_text, (account.available, account.held, total))
            lines.append(f"{client},{','.join(balances)},{str(account.frozen).lower()}")
        return "\n".join(lines) + "\n"


def funds_text(units):
    sign = "-" if units < 0 else ""
    whole, fraction = divmod(abs(units), 10_000)
    return f"{sign}{whole}.{fraction:04d}"


def scenario(a, b, c, t):
    """Gives rows that cause each outcome. A person can calculate these rows by hand."""
    return [
        # Normal events. The program rejects the withdrawal of b, but t5 stays used.
        f"deposit, {a}, {t(1)}, 10.0",
        f"deposit, {b}, {t(2)}, 2.0",
        f"deposit, {a}, {t(3)}, 2.5",
        f"withdrawal, {a}, {t(4)}, 1.5",
        f"withdrawal, {b}, {t(5)}, 3.0",
        # The owner uses an ID again. Then other clients use the ID.
        f"deposit, {a}, {t(1)}, 5.0",
        f"deposit, {b}, {t(5)}, 1.0",
        f"deposit, {b}, {t(1)}, 5.0",
        f"withdrawal, {c}, {t(1)}, 1.0",
        # These events must not change funds.
        f"dispute, {b}, {t(1)},",
        f"resolve, {a}, {t(1)},",
        f"chargeback, {a}, {t(1)},",
        f"dispute, {a}, {t(99)},",
        f"dispute, {c}, {t(99)},",
        f"dispute, {b}, {t(5)},",
        f"dispute, {a}, {t(5)},",
        # A disputed deposit. The client cannot withdraw held funds.
        f"dispute, {a}, {t(1)},",
        f"dispute, {a}, {t(1)},",
        f"withdrawal, {a}, {t(6)}, 5.0",
        f"resolve, {b}, {t(1)},",
        f"chargeback, {c}, {t(1)},",
        f"resolve, {a}, {t(1)},",
        f"dispute, {a}, {t(1)},",
        f"resolve, {a}, {t(1)},",
        # A disputed withdrawal has a resolve. Then a chargeback gives the money back.
        f"dispute, {a}, {t(4)},",
        f"resolve, {a}, {t(4)},",
        f"dispute, {a}, {t(4)},",
        f"chargeback, {a}, {t(4)},",
        # A frozen account rejects withdrawals only. A chargeback is final.
        f"withdrawal, {a}, {t(7)}, 1.0",
        f"deposit, {a}, {t(8)}, 0.5",
        f"dispute, {a}, {t(4)},",
        f"resolve, {a}, {t(4)},",
        f"chargeback, {a}, {t(4)},",
        f"dispute, {a}, {t(8)},",
        f"chargeback, {a}, {t(8)},",
        # The client spends a deposit. A chargeback follows, and a deposit pays it back.
        f"deposit, {b}, {t(9)}, 1.0",
        f"withdrawal, {b}, {t(10)}, 3.0",
        f"dispute, {b}, {t(2)},",
        f"chargeback, {b}, {t(2)},",
        f"deposit, {b}, {t(11)}, 3.0001",
        # The largest balance. One more unit overflows available, and then the total.
        f"deposit, {c}, {t(12)}, {MAX_AMOUNT}",
        f"deposit, {c}, {t(13)}, 0.0001",
        f"withdrawal, {c}, {t(14)}, 0.0001",
        f"deposit, {c}, {t(15)}, 0.0001",
        f"dispute, {c}, {t(14)},",
        f"resolve, {c}, {t(14)},",
        # A row that is not correct must not use its ID.
        f"deposit, {a}, {t(16)}, 1.23456",
        f"deposit, {a}, {t(16)}, 0.0001",
        f"DEPOSIT, {a}, {t(18)}, 1.0",
        f"deposit, 65536, {t(18)}, 1.0",
        f"deposit, {a}, 4294967296, 1.0",
        f"deposit, {a}, {t(18)},",
        f"deposit, {a}, {t(18)}, 0",
        f"deposit, {a}, {t(18)}, -1.0",
        f"deposit, {a}, {t(18)}, 1e3",
        f'deposit, {a}, {t(18)}, "1.0"',
        f"dispute, {a}, {t(1)}, 1.0",
        f"deposit, {a}, {t(18)}, 1,000.50",
        "complete garbage",
        f"deposit, {a}, {t(18)}, \udcff",
        f"deposit, {a}, {t(18)}, 1.0",
    ]


def malformed_row(rng, client, tx):
    """Gives a row that is not correct. Some of these rows are almost correct."""
    templates = [
        b"transfer,%c,%t,1.0",
        b"Deposit,%c,%t,1.0",
        b"WITHDRAWAL,%c,%t,1.0",
        b",%c,%t,1.0",
        b"deposit,,%t,1.0",
        b"deposit,%c,,1.0",
        b"deposit,-1,%t,1.0",
        b"deposit,65536,%t,1.0",
        b"deposit,99999999999,%t,1.0",
        b"deposit,%c,-1,1.0",
        b"deposit,%c,4294967296,1.0",
        b"deposit,%c,1.5,1.0",
        b"deposit,0x10,%t,1.0",
        b"deposit,%c,%t",
        b"deposit,%c,%t,",
        b"deposit,%c,%t,   ",
        b"withdrawal,%c,%t,",
        b"deposit,%c,%t,0",
        b"deposit,%c,%t,0.0000",
        b"deposit,%c,%t,-1.0",
        b"deposit,%c,%t,+1.0",
        b"withdrawal,%c,%t,-5.0",
        b"deposit,%c,%t,1.23456",
        b"deposit,%c,%t,0.00001",
        b"deposit,%c,%t,1.00000",
        b"deposit,%c,%t,922337203685477.5808",
        b"deposit,%c,%t,99999999999999999999999",
        b"deposit,%c,%t,.5",
        b"deposit,%c,%t,1.",
        b"deposit,%c,%t,1e3",
        b"deposit,%c,%t,NaN",
        b"deposit,%c,%t,inf",
        b"deposit,%c,%t,0x10",
        b"deposit,%c,%t,1 000",
        b"deposit,%c,%t,1.2.3",
        b"deposit,%c,%t,1,000.50",
        b"deposit,%c,%t,1.0,extra",
        b"deposit,%c,%t,1.0,",
        b"dispute,%c,%t,,junk",
        b'deposit,%c,%t,"1.0"',
        b'deposit,%c,%t,"5.0',
        b'"deposit",%c,%t,1.0',
        b'deposit,"%c",%t,1.0',
        b"dispute,%c,%t,1.0",
        b"resolve,%c,%t,-9.99",
        b"chargeback,%c,%t,abc",
        b"deposit,%c,%t,\xc2\xa01.0",
        b"deposit,\xe3\x80\x80%c,%t,1.0",
        b"deposit,%c,%t,\x0b1.0",
        b"deposit,%c,%t,1.0\x00",
        b"\xef\xbb\xbfdeposit,%c,%t,1.0",
        b"deposit,%c,\xff,1.0",
        b"deposit,%c,%t,caf\xe9",
        b"deposit,%c,%t,1.0,\xff",
        b"\xff\xfe",
        b"garbage",
        b"   ",
        b",,,",
    ]
    row = rng.choice(templates)
    return row.replace(b"%c", b"%d" % client).replace(b"%t", b"%d" % tx)


class Generator:
    def __init__(self, seed, clients, events):
        self.rng = random.Random(seed)
        self.ledger = Ledger()
        self.rows = []
        self.events = events
        rng = self.rng

        others = rng.sample(range(1, 65535), clients - 2)
        self.universe = [0, 65535] + others
        taken = set(self.universe)
        self.outsiders = [c for c in range(65536) if c not in taken][:1000]
        self.known = self.universe[:3]
        self.pending = self.universe[3:]
        rng.shuffle(self.pending)

        fresh = rng.sample(range(1, 2**32 - 1), events + 100)
        self.fresh = [2**32 - 1, 0] + fresh  # The scenario uses the two boundary IDs. The sample does not contain them.
        self.open = []
        self.disputed = []
        self.charged_back = []
        self.refused = []
        self.frozen = []

    def write_row(self, raw, kind=None, tx=None):
        outcome = self.ledger.row(raw)
        ending = b"\r\n" if self.rng.random() < 0.05 else b"\n"
        self.rows.append(raw + ending)
        if outcome != APPLIED:
            if kind in ("deposit", "withdrawal") and outcome != SKIPPED:
                if outcome not in (DUPLICATE, MISMATCH):
                    self.refused.append(tx)
            return outcome
        if kind in ("deposit", "withdrawal"):
            self.open.append(tx)
        elif kind == "dispute":
            self.disputed.append(tx)
        elif kind == "resolve":
            self.open.append(tx)
        elif kind == "chargeback":
            self.charged_back.append(tx)
            self.frozen.append(self.ledger.owners[tx])
        return outcome

    def emit(self, kind, client, tx, amount=None):
        rng = self.rng
        fields = [kind, str(client), str(tx)]
        if amount is not None:
            fields.append(amount)
        elif rng.random() < 0.7:
            fields.append("")
        style = rng.random()
        if style < 0.1:
            row = ", ".join(fields)
        elif style < 0.13:
            row = " , ".join(f" {field} " for field in fields)
        elif style < 0.15:
            row = ",".join(f"\t{field}\t" for field in fields)
        else:
            row = ",".join(fields)
        return self.write_row(row.encode(), kind, tx)

    def amount_text(self, units):
        whole, fraction = divmod(units, 10_000)
        style = self.rng.random()
        if fraction == 0 and style < 0.5:
            return str(whole)
        text = f"{whole}.{fraction:04d}"
        if style < 0.6:
            return "00" + text
        return text.rstrip("0").rstrip(".") if style < 0.8 and fraction else text

    def small_units(self):
        return self.rng.choice((1, 25, 5_000, 10_000, 12_345, 100_000, 2_500_000))

    def pick(self, entries, status):
        """Gives a random transaction that has this status. Removes old entries from the list."""
        while entries:
            index = self.rng.randrange(len(entries))
            tx = entries[index]
            client = self.ledger.owners[tx]
            transaction = self.ledger.accounts[client].transactions.get(tx)
            if transaction is not None and transaction[2] == status:
                return tx
            entries[index] = entries[-1]
            entries.pop()
        return None

    def any_client(self):
        return self.rng.choice(self.known)

    def other_client(self, client):
        while True:
            other = self.rng.choice(self.universe)
            if other != client:
                return other

    def used_tx(self):
        entries = self.open or self.disputed or self.charged_back
        return self.rng.choice(entries)

    def deposit(self):
        units = self.small_units()
        self.emit("deposit", self.any_client(), self.fresh.pop(), self.amount_text(units))

    def withdrawal(self):
        client = self.any_client()
        available = self.ledger.accounts[client].available
        if available <= 0 or client in self.frozen:
            return self.deposit()
        units = self.rng.randint(1, min(available, 10_000_000))
        self.emit("withdrawal", client, self.fresh.pop(), self.amount_text(units))

    def settle(self, kind, entries, status):
        tx = self.pick(entries, status)
        if tx is None:
            return self.deposit()
        self.emit(kind, self.ledger.owners[tx], tx)

    def dispute(self):
        self.settle("dispute", self.open, "undisputed")

    def resolve(self):
        self.settle("resolve", self.disputed, "disputed")

    def chargeback(self):
        self.settle("chargeback", self.disputed, "disputed")

    def reused_id_by_owner(self):
        tx = self.used_tx()
        kind = self.rng.choice(("deposit", "withdrawal"))
        self.emit(kind, self.ledger.owners[tx], tx, self.amount_text(self.small_units()))

    def reused_id_by_other_client(self):
        tx = self.used_tx()
        kind = self.rng.choice(("deposit", "withdrawal"))
        other = self.other_client(self.ledger.owners[tx])
        self.emit(kind, other, tx, self.amount_text(self.small_units()))

    def event_on_another_clients_transaction(self):
        tx = self.used_tx()
        kind = self.rng.choice(("dispute", "resolve", "chargeback"))
        self.emit(kind, self.other_client(self.ledger.owners[tx]), tx)

    def reference_to_unknown_transaction(self):
        kind = self.rng.choice(("dispute", "resolve", "chargeback"))
        client = self.any_client()
        if self.outsiders and self.rng.random() < 0.5:
            client = self.rng.choice(self.outsiders)
        self.emit(kind, client, self.rng.choice(self.fresh))

    def settle_undisputed(self):
        self.settle(self.rng.choice(("resolve", "chargeback")), self.open, "undisputed")

    def dispute_twice(self):
        self.settle("dispute", self.disputed, "disputed")

    def touch_charged_back(self):
        kind = self.rng.choice(("dispute", "resolve", "chargeback"))
        self.settle(kind, self.charged_back, "charged back")

    def dispute_refused(self):
        if not self.refused:
            return self.deposit()
        tx = self.rng.choice(self.refused)
        self.emit("dispute", self.ledger.owners[tx], tx)

    def overdraw(self):
        client = self.any_client()
        account = self.ledger.accounts[client]
        units = max(account.available, 0) + self.rng.randint(1, 1_000_000)
        self.emit("withdrawal", client, self.fresh.pop(), self.amount_text(units))

    def withdraw_held_funds(self):
        tx = self.pick(self.disputed, "disputed")
        if tx is None:
            return self.overdraw()
        client = self.ledger.owners[tx]
        account = self.ledger.accounts[client]
        units = max(account.available, 0) + self.rng.randint(1, account.held)
        self.emit("withdrawal", client, self.fresh.pop(), self.amount_text(units))

    def on_frozen_account(self):
        if not self.frozen:
            return self.deposit()
        client = self.rng.choice(self.frozen)
        kind = self.rng.choice(("deposit", "withdrawal"))
        self.emit(kind, client, self.fresh.pop(), self.amount_text(self.small_units()))

    def largest_deposit(self):
        self.emit("deposit", self.any_client(), self.fresh.pop(), MAX_AMOUNT)

    def malformed(self):
        # The ID stays available, because a row that is not correct must not use it.
        tx = self.rng.choice(self.fresh)
        self.write_row(malformed_row(self.rng, self.any_client(), tx))

    def blank_line(self):
        self.rows.append(b"\n")

    def open_next_client(self):
        client = self.pending.pop()
        self.known.append(client)
        units = self.small_units()
        self.emit("deposit", client, self.fresh.pop(), self.amount_text(units))

    def play(self):
        steps = [
            (self.deposit, 28),
            (self.withdrawal, 16),
            (self.dispute, 9),
            (self.resolve, 5),
            (self.chargeback, 1),
            (self.reused_id_by_owner, 3),
            (self.reused_id_by_other_client, 3),
            (self.event_on_another_clients_transaction, 4),
            (self.reference_to_unknown_transaction, 3),
            (self.settle_undisputed, 3),
            (self.dispute_twice, 2),
            (self.touch_charged_back, 2),
            (self.dispute_refused, 2),
            (self.overdraw, 3),
            (self.withdraw_held_funds, 2),
            (self.on_frozen_account, 3),
            (self.largest_deposit, 0.02),
            (self.malformed, 7),
        ]
        actions = [action for action, _ in steps]
        weights = [weight for _, weight in steps]

        a, b, c = self.known
        ids = {1: 0, 2: 2**32 - 1}
        self.fresh = self.fresh[:-2]

        def t(number):
            return ids.setdefault(number, self.fresh.pop())

        for row in scenario(a, b, c, t):
            self.write_row(row.encode("utf-8", "surrogateescape"))
        self.refresh_targets_after_scenario()

        while (remaining := self.events - self.row_count()) > 0:
            if self.rng.random() < len(self.pending) / remaining:
                self.open_next_client()
            else:
                self.rng.choices(actions, weights)[0]()
                if self.rng.random() < 0.002:
                    self.blank_line()

    def row_count(self):
        return sum(self.ledger.outcomes.values())

    def refresh_targets_after_scenario(self):
        for tx, client in self.ledger.owners.items():
            transaction = self.ledger.accounts[client].transactions.get(tx)
            if transaction is None:
                self.refused.append(tx)
            elif transaction[2] == "charged back":
                self.charged_back.append(tx)
                self.frozen.append(client)
            else:
                self.open.append(tx)


def check_tiny(ledger):
    """Compares Ledger with results that a person calculated from the rules."""
    assert ledger.accounts_csv() == (
        "client,available,held,total,locked\n"
        "0,13.5001,0.0000,13.5001,true\n"
        "7,922337203685477.5807,0.0000,922337203685477.5807,false\n"
        "65535,1.0001,0.0000,1.0001,true\n"
    ), ledger.accounts_csv()
    expected = {
        APPLIED: 25,
        SKIPPED: 13,
        DUPLICATE: 2,
        MISMATCH: 6,
        INSUFFICIENT: 2,
        UNKNOWN: 3,
        ALREADY_DISPUTED: 1,
        NOT_DISPUTED: 3,
        ALREADY_CHARGED_BACK: 3,
        FROZEN: 1,
        OVERFLOW: 2,
    }
    assert dict(ledger.outcomes) == expected, ledger.outcomes


def write_fixture(name, data):
    directory = Path(__file__).parent / name
    directory.mkdir(exist_ok=True)
    (directory / "transactions.csv").write_bytes(data)

    # Read the file again, so that the expected results come from the written bytes.
    ledger = Ledger()
    for raw in re.split(rb"\r\n|\n|\r", data)[1:]:
        if raw:
            ledger.row(raw)
    (directory / "accounts.csv").write_text(ledger.accounts_csv())
    outcomes = [f"{SKIPPED},{ledger.outcomes[SKIPPED]}"]
    outcomes += [f"{outcome},{ledger.outcomes[outcome]}" for outcome in REJECTIONS]
    (directory / "outcomes.csv").write_text("\n".join(outcomes) + "\n")
    return ledger


def generate_tiny():
    ids = {1: 0, 2: 4294967295}

    def t(number):
        return ids.setdefault(number, number + 100)

    rows = scenario(0, 65535, 7, t)
    data = HEADER + b"".join(
        row.encode("utf-8", "surrogateescape") + b"\n" for row in rows
    )
    ledger = write_fixture("tiny", data)
    check_tiny(ledger)
    print(f"tiny: {sum(ledger.outcomes.values())} events, {len(ledger.accounts)} accounts")


def generate(name, seed, clients, events):
    generator = Generator(seed, clients, events)
    generator.play()
    rows = generator.rows
    rows[-1] = rows[-1].rstrip(b"\r\n")  # Remove the last line ending, because a file can stop without one.
    ledger = write_fixture(name, HEADER + b"".join(rows))

    assert sum(ledger.outcomes.values()) == events
    assert ledger.outcomes == generator.ledger.outcomes
    assert len(ledger.accounts) == clients
    assert all(ledger.outcomes[outcome] for outcome in REJECTIONS + (SKIPPED, APPLIED))
    print(f"{name}: {events} events, {clients} accounts, {dict(ledger.outcomes)}")


if __name__ == "__main__":
    generate_tiny()
    generate("small", seed=1, clients=25, events=1_000)
    generate("medium", seed=2, clients=1_000, events=25_000)
    generate("large", seed=3, clients=10_000, events=100_000)
    generate("max", seed=4, clients=65_536, events=500_000)
