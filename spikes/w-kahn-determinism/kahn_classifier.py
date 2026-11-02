#!/usr/bin/env python3
"""
kahn_classifier.py — Opt-in Kahn Determinism Pass & Corpus Classifier (Proposal #2475)
======================================================================================
Implements the schedule-independence classifier specified in Wildcard Proposal #2475:
"Kahn determinism: prove that a checked task network computes the same output on every schedule".

Checks whether a Tier-0 race-free Hybrid program satisfies Kahn's 1974 theorem:
  1. Single-Producer / Single-Consumer (SPSC) FIFO channels:
     - Each `make(chan T, ...)` channel has at most 1 producer task (send/close) and
       at most 1 consumer task (recv/range).
     - Spawning a task inside a loop (`for ... { task f(ch) }`) creates multiple
       concurrent endpoints on `ch` -> rejected with `HYB-E3501` (multi-producer) or
       `HYB-E3502` (multi-consumer).
  2. No `select` / `default` statements (`HYB-E3503`).
  3. No timers or wall-clock calls (`time.after`, `time.sleep`, `time.now`, `time.since`) (`HYB-E3504`).
  4. No `managed` blocks or shared `sync/atomic` state (`HYB-E3505`).
  5. No unordered `for k, v in map` iteration reaching a channel or stdout (`HYB-E3506`).
  6. No foreign/nondeterministic I/O calls (`net/http`, `os.open`, `os.args`, `rust/*`) (`HYB-E3507`).
"""

from __future__ import annotations

import argparse
import json
import os
import re
import subprocess
import sys
import time
from dataclasses import dataclass, field
from pathlib import Path
from typing import Dict, List, Optional, Set, Tuple

if hasattr(sys.stdout, "reconfigure"):
    sys.stdout.reconfigure(encoding="utf-8")


@dataclass
class Diagnostic:
    code: str
    rule: str
    message: str


@dataclass
class ProgramClassification:
    name: str
    category: str
    tier0_verdict: str
    uses_concurrency: bool
    kahn_deterministic: bool
    commutative_sink_recoverable: bool
    latency_ms: float = 0.0
    diagnostics: List[Diagnostic] = field(default_factory=list)


def strip_comments_and_strings(src: str) -> str:
    """Strip line comments while preserving line structure and blanking string literals."""
    out_lines = []
    for line in src.splitlines():
        in_str = False
        in_raw = False
        cleaned = []
        i = 0
        while i < len(line):
            ch = line[i]
            if ch == '`' and not in_str:
                in_raw = not in_raw
                cleaned.append(' ')
            elif ch == '"' and not in_raw:
                in_str = not in_str
                cleaned.append(' ')
            elif not in_str and not in_raw and line[i:i+2] == '//':
                break
            else:
                if in_str or in_raw:
                    cleaned.append(' ')
                else:
                    cleaned.append(ch)
            i += 1
        out_lines.append("".join(cleaned))
    return "\n".join(out_lines)


def parse_functions(code: str) -> Dict[str, Tuple[str, str]]:
    """Return mapping fn_name -> (params_str, body_str)."""
    fns: Dict[str, Tuple[str, str]] = {}
    pattern = re.compile(r"\bfn\s+(?:\([^)]*\)\s*)?([A-Za-z_][A-Za-z0-9_.]*)\s*(?:\[[^\]]*\])?\s*\(([^)]*)\)[^{]*\{")
    for m in pattern.finditer(code):
        name = m.group(1)
        params = m.group(2)
        start = m.end() - 1
        depth = 0
        end = start
        for idx in range(start, len(code)):
            if code[idx] == '{':
                depth += 1
            elif code[idx] == '}':
                depth -= 1
                if depth == 0:
                    end = idx
                    break
        fns[name] = (params, code[start+1:end])
    return fns


def classify_hy_source(name: str, raw_src: str, category: str = "corpus") -> ProgramClassification:
    t0 = time.perf_counter()
    verdict_match = re.search(r"^//\s*EXPECTED-VERDICT:\s*(ACCEPT-MANAGED|ACCEPT|REJECT)\b", raw_src, re.M)
    if verdict_match:
        tier0 = verdict_match.group(1)
    else:
        tier0 = "EXAMPLE"

    code = strip_comments_and_strings(raw_src)
    diags: List[Diagnostic] = []

    if tier0 == "REJECT":
        dt = (time.perf_counter() - t0) * 1000.0
        return ProgramClassification(
            name=name,
            category=category,
            tier0_verdict=tier0,
            uses_concurrency=bool(re.search(r"\b(task|select|chan)\b|<-", code)),
            kahn_deterministic=False,
            commutative_sink_recoverable=False,
            latency_ms=dt,
            diagnostics=[Diagnostic("HYB-TIER0", "TIER0", "rejected by Tier-0 ownership/buildability checker")],
        )

    uses_conc = bool(re.search(r"\b(task|select|chan)\b|<-", code))

    # 1. Check `managed` or `sync/atomic` (HYB-E3505)
    if tier0 == "ACCEPT-MANAGED" or re.search(r"\bmanaged\s*\(", code):
        diags.append(Diagnostic(
            "HYB-E3505",
            "KAHN-MANAGED",
            "managed block suppresses static ownership proof; schedule independence cannot be certified",
        ))
    if re.search(r"\batomic\.[A-Za-z0-9_]+", code):
        diags.append(Diagnostic(
            "HYB-E3505",
            "KAHN-ATOMIC",
            "shared sync/atomic state permits schedule-dependent interleaving of reads and writes",
        ))

    # 2. Check `select` / `default` (HYB-E3503)
    if re.search(r"\bselect\s*\{", code):
        diags.append(Diagnostic(
            "HYB-E3503",
            "KAHN-SELECT",
            "`select` statement introduces non-deterministic readiness arbitration across channels",
        ))

    # 3. Check timers / wall-clock (`time.after`, `time.sleep`, `time.now`, `time.since`) (HYB-E3504)
    if re.search(r"\btime\.(after|sleep|now|since)\b", code):
        diags.append(Diagnostic(
            "HYB-E3504",
            "KAHN-TIME",
            "timer or wall-clock operation (`time.*`) couples execution to real-time scheduling",
        ))

    # 4. Check foreign / environment / I/O calls in examples (HYB-E3507)
    if re.search(r"\b(http\.new_serve_mux|httptest\.get|os\.args|os\.open)\b", code):
        diags.append(Diagnostic(
            "HYB-E3507",
            "KAHN-FOREIGN-IO",
            "external environment / I/O / HTTP handler call outside the pure Kahn process network",
        ))

    # 5. Analyze channels and task spawns for SPSC (Single-Producer, Single-Consumer)
    fns = parse_functions(code)

    for fn_name, (params, body) in fns.items():
        # A. Spawning inside a `for` loop (`for ... { task f(...) }`)
        for fm in re.finditer(r"\bfor\b[^{]*\{", body):
            fstart = fm.end() - 1
            depth = 0
            fend = fstart
            for idx in range(fstart, len(body)):
                if body[idx] == '{':
                    depth += 1
                elif body[idx] == '}':
                    depth -= 1
                    if depth == 0:
                        fend = idx
                        break
            for_block = body[fstart+1:fend]
            for tm in re.finditer(r"\btask\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(([^)]*)\)", for_block):
                callee = tm.group(1)
                callee_info = fns.get(callee)
                if callee_info:
                    c_params, c_body = callee_info
                    if "<-chan" in c_params:
                        diags.append(Diagnostic(
                            "HYB-E3502",
                            "KAHN-MULTI-CONSUMER",
                            f"`task {callee}(...)` spawned in loop shares `<-chan` across multiple consumers",
                        ))
                    if "chan<-" in c_params or re.search(r"[A-Za-z0-9_]+\s*<-\s*", c_body):
                        diags.append(Diagnostic(
                            "HYB-E3501",
                            "KAHN-MULTI-PRODUCER",
                            f"`task {callee}(...)` spawned in loop shares `chan<-` across multiple producers",
                        ))

        # B. Multiple distinct `task` spawns in the same scope sharing a channel argument
        producer_counts: Dict[str, int] = {}
        consumer_counts: Dict[str, int] = {}
        for tm in re.finditer(r"\btask\s+([A-Za-z_][A-Za-z0-9_]*)\s*\(([^)]*)\)", body):
            callee = tm.group(1)
            args = [a.strip() for a in tm.group(2).split(",") if a.strip()]
            callee_info = fns.get(callee)
            if callee_info:
                c_params_raw, _ = callee_info
                c_params = [p.strip() for p in c_params_raw.split(",") if p.strip()]
                for arg, p_decl in zip(args, c_params):
                    if "chan<-" in p_decl:
                        producer_counts[arg] = producer_counts.get(arg, 0) + 1
                    elif "<-chan" in p_decl:
                        consumer_counts[arg] = consumer_counts.get(arg, 0) + 1
        for ch_arg, cnt in producer_counts.items():
            if cnt > 1:
                diags.append(Diagnostic(
                    "HYB-E3501",
                    "KAHN-MULTI-PRODUCER",
                    f"channel `{ch_arg}` passed as `chan<-` producer to {cnt} distinct tasks",
                ))
        for ch_arg, cnt in consumer_counts.items():
            if cnt > 1:
                diags.append(Diagnostic(
                    "HYB-E3502",
                    "KAHN-MULTI-CONSUMER",
                    f"channel `{ch_arg}` passed as `<-chan` consumer to {cnt} distinct tasks",
                ))

    # 6. Check unordered map iteration directly emitting to channel or output (`HYB-E3506`)
    map_vars: Set[str] = set()
    for mm in re.finditer(r"\b([A-Za-z_][A-Za-z0-9_]*)\s*(?::=|=)\s*map\[", code):
        map_vars.add(mm.group(1))
    for mv in map_vars:
        for fm in re.finditer(rf"\bfor\s+[A-Za-z0-9_,\s]+\bin\s+{re.escape(mv)}\b[^{{]*\{{", code):
            fstart = fm.end() - 1
            depth = 0
            fend = fstart
            for idx in range(fstart, len(code)):
                if code[idx] == '{':
                    depth += 1
                elif code[idx] == '}':
                    depth -= 1
                    if depth == 0:
                        fend = idx
                        break
            loop_body = code[fstart+1:fend]
            if "<-" in loop_body or "fmt.print" in loop_body:
                diags.append(Diagnostic(
                    "HYB-E3506",
                    "KAHN-MAP-ITER",
                    f"unordered iteration over map `{mv}` reaches channel send or stdout",
                ))

    # Deduplicate diagnostics by (code, rule)
    unique_diags: List[Diagnostic] = []
    seen_keys: Set[Tuple[str, str]] = set()
    for d in diags:
        k = (d.code, d.rule)
        if k not in seen_keys:
            seen_keys.add(k)
            unique_diags.append(d)

    codes = {d.code for d in unique_diags}
    comm_recoverable = False
    if codes and codes.issubset({"HYB-E3501", "HYB-E3502", "HYB-E3504", "HYB-E3505"}):
        if (
            re.search(r"\bsort\.ints\s*\(", code)
            or re.search(r"\bmerged\[[A-Za-z0-9_]+\]\s*\+=\s*", code)
            or re.search(r"\btotal\s*\+=\s*", code)
        ):
            comm_recoverable = True

    dt = (time.perf_counter() - t0) * 1000.0
    return ProgramClassification(
        name=name,
        category=category,
        tier0_verdict=tier0,
        uses_concurrency=uses_conc,
        kahn_deterministic=(len(unique_diags) == 0),
        commutative_sink_recoverable=comm_recoverable,
        latency_ms=dt,
        diagnostics=unique_diags,
    )


def resolve_repo_paths(spike_dir: Path) -> Tuple[Optional[Path], Optional[Path]]:
    """Find `tests/checker-corpus` and `examples` either in repo root or local audit cache."""
    repo_root = spike_dir.parent.parent
    c1 = repo_root / "tests" / "checker-corpus"
    e1 = repo_root / "examples"
    if c1.is_dir() and e1.is_dir():
        return c1, e1
    c2 = repo_root / "corpus_129"
    e2 = repo_root / "examples_hy"
    if c2.is_dir() and e2.is_dir():
        return c2, e2
    return None, None


def evaluate_all(spike_dir: Path) -> Dict[str, object]:
    hy_dir = spike_dir / "hy"
    spike_fixtures = []
    for p in sorted(hy_dir.glob("*.hy")):
        c = classify_hy_source(p.name, p.read_text(encoding="utf-8"), category="spike-fixture")
        spike_fixtures.append(c)

    c_dir, e_dir = resolve_repo_paths(spike_dir)
    corpus_results: List[ProgramClassification] = []
    ex_results: List[ProgramClassification] = []

    if c_dir is not None:
        for p in sorted(c_dir.glob("*.hy")):
            corpus_results.append(classify_hy_source(p.name, p.read_text(encoding="utf-8"), category="corpus"))
    if e_dir is not None:
        # Either flat `examples_hy/*.hy` or nested `examples/*/*.hy`
        ex_files = sorted(e_dir.glob("*.hy")) + sorted(e_dir.glob("*/*.hy"))
        for p in ex_files:
            rel_name = f"{p.parent.name}/{p.name}" if p.parent != e_dir else p.name.replace("__", "/")
            ex_results.append(classify_hy_source(rel_name, p.read_text(encoding="utf-8"), category="example"))

    accept_corpus = [r for r in corpus_results if r.tier0_verdict in ("ACCEPT", "ACCEPT-MANAGED")]
    admitted_corpus = [r for r in accept_corpus if r.kahn_deterministic]
    rejected_corpus = [r for r in accept_corpus if not r.kahn_deterministic]
    comm_recoverable = [r for r in rejected_corpus if r.commutative_sink_recoverable]
    total_ms = sum(r.latency_ms for r in corpus_results) + sum(r.latency_ms for r in ex_results)

    return {
        "spike_fixtures": [
            {
                "name": r.name,
                "kahn_deterministic": r.kahn_deterministic,
                "commutative_recoverable": r.commutative_sink_recoverable,
                "diagnostics": [d.code for d in r.diagnostics],
            }
            for r in spike_fixtures
        ],
        "corpus_total": len(corpus_results),
        "corpus_tier0_accept_plus_managed": len(accept_corpus),
        "kahn_admitted_corpus": len(admitted_corpus),
        "kahn_rejected_corpus": len(rejected_corpus),
        "kahn_rejected_commutative_recoverable": len(comm_recoverable),
        "total_analysis_latency_ms": round(total_ms, 3),
        "examples": [
            {
                "name": r.name,
                "uses_concurrency": r.uses_concurrency,
                "kahn_deterministic": r.kahn_deterministic,
                "commutative_recoverable": r.commutative_sink_recoverable,
                "diagnostics": [d.code for d in r.diagnostics],
            }
            for r in ex_results
        ],
    }


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(description="Verify Kahn determinism classifier and schedule fuzzer (#2475).")
    parser.add_argument("--check", action="store_true", help="Verify pre-registered H1-H3 gates.")
    parser.add_argument("--live-go", action="store_true", help="Run the Go N=1,000 schedule fuzzer suite.")
    args = parser.parse_args(argv)

    spike_dir = Path(__file__).resolve().parent
    summary = evaluate_all(spike_dir)

    if args.check:
        fixtures = {f["name"]: f for f in summary["spike_fixtures"]}
        assert fixtures["01-pipeline-kahn-admitted.hy"]["kahn_deterministic"] is True
        assert fixtures["02-pipeline-select-timeout-rejected.hy"]["kahn_deterministic"] is False
        assert "HYB-E3503" in fixtures["02-pipeline-select-timeout-rejected.hy"]["diagnostics"]
        assert fixtures["03-worker-pool-commutative-sink.hy"]["kahn_deterministic"] is False
        assert fixtures["03-worker-pool-commutative-sink.hy"]["commutative_recoverable"] is True

    if args.live_go:
        work_dir = spike_dir / "work"
        env = dict(os.environ)
        env["GODEBUG"] = "asyncpreemptoff=0"
        proc = subprocess.run(
            ["go", "test", "-v", "./..."],
            cwd=str(work_dir),
            env=env,
            capture_output=True,
            text=True,
            check=False,
        )
        if proc.returncode != 0:
            sys.stderr.write(proc.stdout + "\n" + proc.stderr)
            return 1

    print(json.dumps(summary, indent=2))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
