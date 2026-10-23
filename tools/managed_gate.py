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
import os
import re
import subprocess
import sys
import tempfile

VALID = {"mutex", "atomic", "actor", "finalizer", "interop", "other"}


def _mask_comments_and_strings(src: str) -> str:
    """Replace comments and string/char literals with spaces while preserving newlines."""
    out = []
    i = 0
    n = len(src)
    while i < n:
        ch = src[i]
        # Line comment // ...
        if ch == "/" and i + 1 < n and src[i + 1] == "/":
            while i < n and src[i] != "\n":
                out.append(" ")
                i += 1
            continue
        # Block comment /* ... */ (non-nested per LX8)
        if ch == "/" and i + 1 < n and src[i + 1] == "*":
            out.append("  ")
            i += 2
            while i < n and not (src[i] == "*" and i + 1 < n and src[i + 1] == "/"):
                out.append("\n" if src[i] == "\n" else " ")
                i += 1
            if i < n:
                out.append("  ")
                i += 2
            continue
        # Raw string `...`
        if ch == "`":
            out.append(" ")
            i += 1
            while i < n and src[i] != "`":
                out.append("\n" if src[i] == "\n" else " ")
                i += 1
            if i < n:
                out.append(" ")
                i += 1
            continue
        # Quoted string "..." or char literal '...'
        if ch in ('"', "'"):
            quote = ch
            out.append(" ")
            i += 1
            while i < n and src[i] != quote:
                if src[i] == "\\" and i + 1 < n:
                    out.append("\n" if src[i] == "\n" else " ")
                    out.append("\n" if src[i + 1] == "\n" else " ")
                    i += 2
                    continue
                out.append("\n" if src[i] == "\n" else " ")
                i += 1
            if i < n:
                out.append(" ")
                i += 1
            continue
        out.append(ch)
        i += 1
    return "".join(out)


def scan_managed_lines(src: str) -> list[int]:
    """Return 1-based line numbers of lexical `managed (` occurrences outside comments/strings."""
    masked = _mask_comments_and_strings(src)
    return [
        1 + masked.count("\n", 0, m.start())
        for m in re.finditer(r"\bmanaged\s*\(", masked)
    ]


def audit(hyb, prog):
    r = subprocess.run(
        [hyb, "check", prog, "--json", "--report-managed"],
        capture_output=True, timeout=120,
    )
    # The checker serializes UTF-8 JSON regardless of the host locale. Decode
    # bytes here, rather than in Windows subprocess reader threads, so malformed
    # output is a gate error and cannot corrupt a reviewed Unicode baseline.
    try:
        stdout = r.stdout.decode("utf-8")
        stderr = r.stderr.decode("utf-8")
    except UnicodeDecodeError as error:
        print(f"GATE ERROR: {prog}: checker output is not valid UTF-8: {error}")
        sys.exit(2)
    # Fail closed on a checker failure (issue #53): a rejected or malformed
    # program has no trustworthy managed-block inventory. Treating that as
    # "zero blocks" reported PASS and — in --rebaseline mode — could replace
    # a reviewed baseline with an empty one. The gate must stop instead; the
    # rebaseline path flows through this same audit() call.
    if r.returncode != 0:
        print(f"GATE ERROR: {prog}: hyb check exited {r.returncode}; no gate decision on a failed check\n{stderr}")
        sys.exit(2)
    try:
        out = json.loads(stdout)
    except json.JSONDecodeError:
        print(f"GATE ERROR: {prog}: hyb check did not emit JSON\n{stdout}\n{stderr}")
        sys.exit(2)
    # Missing audit data is not evidence of an empty inventory. In particular,
    # do not let a stale/wrapped checker returning {} clear a reviewed baseline.
    summary = out.get("summary") if isinstance(out, dict) else None
    rep = summary.get("managed") if isinstance(summary, dict) else None
    if not isinstance(rep, dict):
        print(f"GATE ERROR: {prog}: missing or malformed summary.managed report")
        sys.exit(2)
    # The Rust serializer omits an empty blocks Vec, but always emits its count.
    reported = rep.get("blocks", [])
    count = rep.get("managed_blocks")
    if (type(count) is not int or count < 0 or not isinstance(reported, list)
            or count != len(reported)):
        print(f"GATE ERROR: {prog}: malformed managed block list or inconsistent count")
        sys.exit(2)
    blocks = []
    for b in reported:
        category = b.get("category") if isinstance(b, dict) else None
        if not isinstance(category, str) or category not in VALID:
            print(f"GATE ERROR: {prog}: category {category!r} outside the closed set")
            sys.exit(2)
        blocks.append(b)
    # line numbers: lexically scan the source for `managed (` tokens outside
    # comments and string literals (issue #119; the JSON report is
    # source-ordered, so pairing by lexical order is stable for a single file).
    with open(prog, encoding="utf-8") as f:
        lines = scan_managed_lines(f.read())
    # Never guess a location or collapse several reports into a shared `?`
    # key, and reject multiple managed blocks on the same line (file:line keys
    # cannot distinguish them).
    if len(lines) != len(blocks) or len(set(lines)) != len(lines):
        print(
            f"GATE ERROR: {prog}: {len(blocks)} reported managed block(s) "
            f"but {len(lines)} unambiguous source location(s) ({len(set(lines))} distinct line(s)); "
            "cannot pair them uniquely. Put each managed block on its own line."
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
        with open(baseline_path, encoding="utf-8") as f:
            baseline = json.load(f)
    except FileNotFoundError:
        baseline = {"version": 1, "blocks": {}}
    except (json.JSONDecodeError, UnicodeDecodeError) as e:
        print(f"GATE ERROR: {baseline_path}: malformed baseline JSON: {e}")
        sys.exit(2)

    if not isinstance(baseline, dict):
        print(f"GATE ERROR: {baseline_path}: baseline must be a JSON object, got {type(baseline).__name__}")
        sys.exit(2)

    base = baseline.setdefault("blocks", {})
    if not isinstance(base, dict):
        print(f"GATE ERROR: {baseline_path}: baseline blocks must be a JSON object, got {type(base).__name__}")
        sys.exit(2)

    current = {}
    for prog in progs:
        current.update(audit(hyb, prog))

    # pr-434: scope the whole decision to the programs actually audited in
    # this run. Baseline blocks belonging to unaudited programs are neither
    # evidence of a removal (the informational `removed` list) nor disposable
    # in --rebaseline — a subset run must not silently rewrite (and drop)
    # reviewed blocks for programs it never scanned.
    audited_progs = set(progs)
    new = {k: v for k, v in current.items() if k not in base}
    removed = {
        k for k in base
        if k.rsplit(":", 1)[0] in audited_progs and k not in current
    }
    changed = {
        k
        for k in current
        if k in base and base[k] != current[k]
    }

    if rebaseline:
        # pr-434 scope rule: keep every reviewed block whose program was not
        # audited in this run; the audited programs get the fresh inventory.
        preserved = {
            k: v for k, v in base.items()
            if k.rsplit(":", 1)[0] not in audited_progs
        }
        preserved.update(current)
        baseline["blocks"] = preserved
        # Never truncate the reviewed baseline until the complete replacement
        # has been written and closed successfully. Buffered writes can fail
        # at close (e.g. ENOSPC/EFBIG), after json.dump itself has returned.
        # Resolve symlinks so rebaselining keeps updating their target.
        target = os.path.realpath(baseline_path)
        temporary = None
        try:
            with tempfile.NamedTemporaryFile(
                mode="w", encoding="utf-8", dir=os.path.dirname(target),
                prefix="." + os.path.basename(target) + ".", delete=False,
            ) as output:
                temporary = output.name
                if os.path.exists(target):
                    os.chmod(temporary, os.stat(target).st_mode & 0o777)
                json.dump(baseline, output, indent=1, sort_keys=True)
                output.write("\n")
                output.flush()
                os.fsync(output.fileno())
            os.replace(temporary, target)
            temporary = None
        except OSError as error:
            print(f"GATE ERROR: cannot replace baseline {baseline_path}: {error}")
            sys.exit(2)
        finally:
            if temporary is not None:
                os.unlink(temporary)
        print(f"rebaseline: {len(preserved)} block(s) recorded in {baseline_path}")
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
