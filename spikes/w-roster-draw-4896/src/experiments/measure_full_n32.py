"""Single, isolated measurement of the n=32 instance with the *age-carrying* state.

Why a separate process: that variant needs roughly 400k memoised frozensets on
this family, which is enough to fail with ``MemoryError`` when the main
experiment's heap is already large.  Running it alone makes the measurement
reproducible and keeps one heavy row from killing the whole report.

    python src/experiments/measure_full_n32.py

Writes ``out/full_n32.json``.
"""

from __future__ import annotations

import gc
import json
import sys
import time
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "src"))

from hybrid_roster.determinize import SetAutomaton
from hybrid_roster.policy import scale_policy

N = 32


def main() -> None:
    policy = scale_policy(N)
    gc.collect()
    started = time.perf_counter()
    try:
        automaton = SetAutomaton(policy)
        total = automaton.denominator
        elapsed = time.perf_counter() - started
        record = {
            "n": N,
            "ok": True,
            "visible_words": total,
            "bits": total.bit_length(),
            "seconds": round(elapsed, 3),
            "distinct_sets": automaton.distinct_sets(),
            "count_entries": len(automaton._count),
            "delta_entries": len(automaton._delta),
            "peak_members": automaton.peak_members,
            "candidate_space": str(policy.words()),
        }
    except MemoryError:
        record = {
            "n": N,
            "ok": False,
            "error": "MemoryError",
            "seconds": round(time.perf_counter() - started, 3),
            "candidate_space": str(policy.words()),
        }
    out = ROOT / "out"
    out.mkdir(exist_ok=True)
    (out / "full_n32.json").write_text(json.dumps(record, indent=2), encoding="utf-8")
    print(json.dumps(record, indent=2))


if __name__ == "__main__":
    main()
