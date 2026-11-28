#!/usr/bin/env python3
"""
falsify.py — run the #2475 H1 falsifier against the REAL pipeline.

The merged spike (PR #2481) tested H1 on hand-written Go transcriptions.
This script tests it the way the pre-registration says: every program goes
through `hyb check` -> `hyb build` -> the emitted binary, run N times under
GOMAXPROCS 1..8 and GODEBUG=asyncpreemptoff=0.

For each program in hy/falsifiers/ it reports:
  * the Kahn classifier verdict (kahn_classifier.classify_hy_source)
  * the hy-check verdict
  * how many distinct stdout values N runs produced

H1 is falsified by any program that is (classifier ADMIT) AND (hy-check
ACCEPT) AND (>= 2 distinct outputs).

Usage (from the repo root, after `cd compiler && cargo build -p hyb`):
  python3 spikes/w-kahn-determinism/falsify.py --bin compiler/target/debug/hyb
  python3 spikes/w-kahn-determinism/falsify.py --bin <hyb> --runs 1000 --json out.json
"""
from __future__ import annotations

import argparse
import collections
import json
import os
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

SPIKE = Path(__file__).resolve().parent
sys.path.insert(0, str(SPIKE))
from kahn_classifier import classify_hy_source  # noqa: E402


def hy_check(bin_path: str, hy: Path) -> tuple[str, str]:
    p = subprocess.run([bin_path, "check", str(hy), "--json"],
                       capture_output=True, timeout=60)
    out = p.stdout.decode("utf-8", "replace").lstrip("﻿").strip()
    try:
        payload = json.loads(out)
    except json.JSONDecodeError:
        return "ERROR", (p.stderr.decode("utf-8", "replace") or out)[-300:]
    if not isinstance(payload, dict):
        return "ERROR", "checker JSON is not an object"
    verdict = str(payload.get("summary", {}).get("verdict", "")).lower()
    errors = [d for d in payload.get("diagnostics", [])
              if isinstance(d, dict) and d.get("severity") == "error"]
    if p.returncode == 0 and verdict != "reject" and not errors:
        return "ACCEPT", ""
    detail = "; ".join(f"{d.get('code', '?')}/{d.get('tier0_rule', '?')}" for d in errors)
    return "REJECT", (detail or f"verdict={verdict} exit={p.returncode}")[:300]


def build(bin_path: str, hy: Path, workdir: Path) -> Path:
    shutil.copy(hy, workdir / hy.name)
    p = subprocess.run([bin_path, "build", hy.name], cwd=workdir,
                       capture_output=True, text=True, timeout=300)
    if p.returncode != 0:
        raise RuntimeError((p.stderr or p.stdout).strip()[-400:])
    exe = workdir / "a.out"
    if not exe.exists():
        raise RuntimeError("hyb build produced no a.out")
    return exe


def run_many(exe: Path, runs: int) -> collections.Counter:
    outs: collections.Counter = collections.Counter()
    for i in range(runs):
        env = dict(os.environ, GOMAXPROCS=str(1 + i % 8),
                   GODEBUG="asyncpreemptoff=0")
        p = subprocess.run([str(exe)], capture_output=True, text=True,
                           timeout=60, env=env)
        key = p.stdout if p.returncode == 0 else f"<exit {p.returncode}> {p.stderr[-120:]}"
        outs[key] += 1
    return outs


def main() -> int:
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--bin", required=True, help="path to the built hyb binary")
    ap.add_argument("--runs", type=int, default=1000)
    ap.add_argument("--dir", default=str(SPIKE / "hy" / "falsifiers"))
    ap.add_argument("--json", help="write full results here")
    args = ap.parse_args()
    args.bin = str(Path(args.bin).resolve())  # build() runs from a temp dir

    results, falsified = [], 0
    for hy in sorted(Path(args.dir).glob("*.hy")):
        src = hy.read_text(encoding="utf-8")
        cls = classify_hy_source(hy.name, src, category="falsifier")
        admit = cls.kahn_deterministic
        check, why = hy_check(args.bin, hy)
        row = {"program": hy.name, "classifier": "ADMIT" if admit else "REJECT",
               "classifier_diags": [d.code for d in cls.diagnostics],
               "hy_check": check, "hy_check_detail": why,
               "runs": 0, "distinct_outputs": 0, "outputs": {}}
        if check == "ACCEPT":
            with tempfile.TemporaryDirectory() as td:
                try:
                    exe = build(args.bin, hy, Path(td))
                    outs = run_many(exe, args.runs)
                    row.update(runs=args.runs, distinct_outputs=len(outs),
                               outputs=dict(outs.most_common(10)))
                except Exception as e:  # build failures are reported, not hidden
                    row["build_error"] = str(e)
        row["falsifies_H1"] = admit and check == "ACCEPT" and row["distinct_outputs"] >= 2
        falsified += row["falsifies_H1"]
        results.append(row)

        flag = "FALSIFIES H1" if row["falsifies_H1"] else ""
        print(f"{hy.name:40} classifier={row['classifier']:6} hy-check={check:6} "
              f"distinct={row['distinct_outputs']:>4}/{row['runs']:<5} {flag}")
        if row.get("build_error"):
            print(f"    build error: {row['build_error']}")
        if check != "ACCEPT" and why:
            print(f"    hy-check: {why}")
        for out, n in list(row["outputs"].items())[:4]:
            print(f"    {n:>5} x {out.strip()[:70]!r}")

    print(f"\nH1 falsifiers confirmed: {falsified}/{len(results)}")
    if args.json:
        Path(args.json).write_text(json.dumps(results, indent=2), encoding="utf-8")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
