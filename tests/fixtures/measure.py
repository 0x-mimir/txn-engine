#!/usr/bin/env python3
"""Measures the release binary on each fixture and writes a Markdown table.

The script does one run that it does not count, so that the file is in the disk cache.
Then it does RUNS runs. The standard output and the standard error go to files.

Usage: cargo build --release && python3 tests/fixtures/measure.py
"""

import os
import statistics
import subprocess
import sys
import tempfile
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
BINARY = ROOT / "target/release/txn-engine"
RUNS = 20
FIXTURES = (
    ("tiny", 3, 61),
    ("small", 25, 1_000),
    ("medium", 1_000, 25_000),
    ("large", 10_000, 100_000),
    ("max", 65_536, 500_000),
)
# ru_maxrss is in bytes on macOS and in kibibytes on Linux.
MAXRSS_BYTES = 1 if sys.platform == "darwin" else 1024


def run(transactions, directory):
    """Gives the elapsed seconds and the peak memory in bytes of one run."""
    with open(directory / "out", "wb") as stdout, open(directory / "err", "wb") as stderr:
        start = time.perf_counter()
        process = subprocess.Popen([BINARY, transactions], stdout=stdout, stderr=stderr)
        # Use wait4, because it gives the memory of this child only.
        _, status, usage = os.wait4(process.pid, 0)
        elapsed = time.perf_counter() - start
    assert status == 0, transactions
    return elapsed, usage.ru_maxrss * MAXRSS_BYTES


def main():
    print("| Fixture | Clients | Events | Median | Fastest | Slowest | Events/s | Peak memory |")
    print("|---|---:|---:|---:|---:|---:|---:|---:|")
    with tempfile.TemporaryDirectory() as directory:
        for name, clients, events in FIXTURES:
            transactions = ROOT / "tests/fixtures" / name / "transactions.csv"
            run(transactions, Path(directory))
            results = [run(transactions, Path(directory)) for _ in range(RUNS)]
            times = [elapsed for elapsed, _ in results]
            median = statistics.median(times)
            memory = max(peak for _, peak in results) / 2**20
            print(
                f"| `{name}` | {clients:,} | {events:,} | {median * 1000:.2f} ms "
                f"| {min(times) * 1000:.2f} ms | {max(times) * 1000:.2f} ms "
                f"| {events / median:,.0f} | {memory:.1f} MiB |"
            )


if __name__ == "__main__":
    main()
