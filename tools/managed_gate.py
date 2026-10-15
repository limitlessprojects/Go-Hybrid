#!/usr/bin/env python3
"""managed_gate.py — the "new managed block appeared" CI gate (row 14,
discussion #4; sits on top of --deny-managed/--report-managed the way the
reviewer asked: a managed block that is NOT in the committed baseline is a
review event and fails the build; REMOVING a block is always allowed).

Usage:
  python3 tools/managed_gate.py <hyb> <baseline.json> <prog.hy> [more.hy ...]
  python3 tools/managed_gate.py --rebaseline <hyb> <baseline.json> <prog...>

Baseline shape: {"version": 1, "blocks": {"<file>:<line>": {"reason": ...,
"category": ..., "tickets": [...]}}} — keyed by file:line so moved blocks
count as new (deliberate: moving a hatch is a review event too).
"""
import json
import subprocess
import sys

VALID = {"mutex", "atomic", "actor", "finalizer", "interop", "other"}


def audit(hyb, prog):
    r = subprocess.run(
        [hyb, "check", prog, "--json", "--report-managed"],
        capture_output=True, text=True, timeout=120,
    )
    # Fail closed on a checker failure (issue #53): a rejected or malformed
    # program has no trustworthy managed-block inventory. Treating that as
    # "zero blocks" reported PASS and — in --rebaseline mode — could replace
    # a reviewed baseline with an empty one. The gate must stop instead; the
    # rebaseline path flows through this same audit() call.
    if r.returncode != 0:
        print(f"GATE ERROR: {prog}: hyb check exited {r.returncode}; no gate decision on a failed check\n{r.stderr}")
        sys.exit(2)
    try:
        out = json.loads(r.stdout)
    except json.JSONDecodeError:
        print(f"GATE ERROR: {prog}: hyb check did not emit JSON\n{r.stdout}\n{r.stderr}")
        sys.exit(2)
    rep = (out.get("summary") or {}).get("managed") or {}
    blocks = []
    for b in rep.get("blocks", []):
        if b.get("category") not in VALID:
            print(f"GATE ERROR: {prog}: category {b.get('category')!r} outside the closed set")
            sys.exit(2)
        blocks.append(b)
    # line numbers: re-scan the source for the reason literals (the JSON
    # report is source-ordered; pairing by order is stable for a single file)
    lines = []
    with open(prog, encoding="utf-8") as f:
        for ln, text in enumerate(f, 1):
            if "managed(" in text and "//" not in text.split("managed(")[0][-2:]:
                lines.append(ln)
    # Never guess a location or collapse several reports into a shared `?`
    # key. That can hide an inserted block when a later block overwrites it.
    # The current baseline format requires one unambiguous block per line;
    # fail closed when the lightweight source scan cannot establish that.
    if len(lines) != len(blocks):
        print(
            f"GATE ERROR: {prog}: {len(blocks)} reported managed block(s) "
            f"but {len(lines)} source location(s); cannot pair them uniquely. "
            "Put each managed block on its own line and check for source "
            "mentions that confuse the scan before rebaselining."
        )
        sys.exit(2)
    keyed = {}
    for i, b in enumerate(blocks):
        key = f"{prog}:{lines[i]}"
        keyed[key] = b
    return keyed


def main():
    args = sys.argv[1:]
    rebaseline = False
    if args and args[0] == "--rebaseline":
        rebaseline = True
        args = args[1:]

    # An empty source list is a missing scan, not evidence that all managed
    # blocks were removed. Reject it before reading or replacing the baseline.
    if len(args) < 3:
        print(
            "GATE ERROR: supply a checker, a baseline, and at least one source file\n"
            "usage: managed_gate.py [--rebaseline] <hyb> <baseline.json> <prog.hy> [more.hy ...]",
            file=sys.stderr,
        )
        sys.exit(2)
    hyb, baseline_path, progs = args[0], args[1], args[2:]

    try:
        baseline = json.load(open(baseline_path))
    except FileNotFoundError:
        baseline = {"version": 1, "blocks": {}}
    base = baseline.setdefault("blocks", {})

    current = {}
    for prog in progs:
        current.update(audit(hyb, prog))

    new = {k: v for k, v in current.items() if k not in base}
    removed = {k for k in base if k not in current}
    changed = {
        k
        for k in current
        if k in base and base[k] != current[k]
    }

    if rebaseline:
        baseline["blocks"] = current
        json.dump(baseline, open(baseline_path, "w"), indent=1, sort_keys=True)
        open(baseline_path, "a").write("\n")
        print(f"rebaseline: {len(current)} block(s) recorded in {baseline_path}")
        return

    fail = False
    for k, v in sorted(new.items()):
        print(f"NEW MANAGED BLOCK — review required: {k} [{v['category']}] {v['reason']!r}")
        fail = True
    for k in sorted(changed):
        print(f"MANAGED BLOCK CHANGED — review required: {k}")
        fail = True
    for k in sorted(removed):
        print(f"managed block removed (allowed): {k}")

    if fail:
        print(
            "managed gate: FAILED — new/changed managed blocks must land in "
            f"{baseline_path} via `tools/managed_gate.py --rebaseline` WITH review "
            "(the hatch is unsafe-by-default, discussion #4)"
        )
        sys.exit(1)
    print(f"managed gate: PASS ({len(current)} block(s), baseline {len(base)})")


if __name__ == "__main__":
    main()
