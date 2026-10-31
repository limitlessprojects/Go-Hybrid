#!/usr/bin/env python3
"""check_claims_links.py — verify the evidence links cited by GUARANTEES.md.

Every row of the claims table cites evidence: markdown links to specs,
corpus programs and test files, sometimes with a `(reject)` / `(accept)` /
`(managed)` marker. Nothing currently checks that those citations still
resolve, or that a cited corpus program still carries the verdict the row
claims it for. This tool closes that gap:

  * every relative markdown link in the claims table must resolve to a file
    (http(s) links and pure anchors are skipped),
  * a corpus link followed by `(reject)`, `(accept)` or `(managed)` must point
    at a file whose `EXPECTED-VERDICT` header agrees with the marker,
  * a cited corpus program with no verdict header at all is an error.

Exit 0 when every citation checks out, 1 otherwise. `--json` prints a
machine-readable report. Stdlib only.
"""
import argparse
import json
import re
import sys
from pathlib import Path

TABLE_HEADING = "## The claims table"
ROW_RE = re.compile(r"^\|\s*(\d+)\s*\|")
LINK_RE = re.compile(r"\[([^\]]*)\]\(([^)\s]+)(?:\s+\"[^\"]*\")?\)")
INLINE_CORPUS_RE = re.compile(r"`(\d{2}-[a-z0-9][a-z0-9-]*(?:\.hy)?)`")
CORPUS_NUM_RE = re.compile(r"\bcorpus\s+(\d{1,3})\b")
MARKER_RE = re.compile(r"^\s*\((reject|accept|managed)\)", re.IGNORECASE)
VERDICT_RE = re.compile(r"EXPECTED-VERDICT:\s*([A-Za-z-]+)")
CHECKED_SUFFIXES = {
    ".md", ".py", ".rs", ".go", ".hy", ".yml", ".yaml", ".json", ".txt", ".toml",
}


def table_rows(text):
    """Yield (row_number, evidence_cell) for every numeric row of the claims table."""
    in_table = False
    for line in text.splitlines():
        stripped = line.strip()
        if stripped.startswith("## "):
            in_table = stripped.startswith(TABLE_HEADING)
            continue
        if not in_table:
            continue
        match = ROW_RE.match(stripped)
        if not match:
            continue
        cells = stripped.split("|")
        # | # | surface | checker | claim | evidence |
        if len(cells) < 6:
            continue
        yield int(match.group(1)), "|".join(cells[5:-1]).strip()


def is_checkable(target):
    if not target or target.startswith(("#", "http://", "https://", "mailto:")):
        return False
    path = target.split("#", 1)[0]
    if not path:
        return False
    return "/" in path or Path(path).suffix.lower() in CHECKED_SUFFIXES


def verdict_of(path):
    try:
        head = path.read_text(encoding="utf-8", errors="replace")[:4000]
    except OSError:
        return None
    match = VERDICT_RE.search(head)
    return match.group(1).upper() if match else None


def marker_matches(marker, verdict):
    if verdict is None:
        return None
    if marker == "reject":
        return verdict == "REJECT"
    if marker == "accept":
        return verdict.startswith("ACCEPT")
    if marker == "managed":
        return verdict == "ACCEPT-MANAGED"
    return True


def check(root):
    claims_file = root / "GUARANTEES.md"
    issues = []
    rows = 0
    links = 0
    refs = 0
    if not claims_file.is_file():
        return {"ok": False, "rows": 0, "links": 0,
                "issues": [{"row": 0, "kind": "missing-claims-file", "target": str(claims_file)}]}
    text = claims_file.read_text(encoding="utf-8", errors="replace")
    for row_number, evidence in table_rows(text):
        rows += 1
        for index, match in enumerate(LINK_RE.finditer(evidence)):
            target = match.group(2)
            if not is_checkable(target):
                continue
            links += 1
            path = target.split("#", 1)[0]
            resolved = root / path
            if not resolved.is_file():
                issues.append({"row": row_number, "kind": "missing", "target": path})
                continue
            if resolved.suffix.lower() != ".hy":
                continue
            follow = evidence[match.end():]
            marker_match = MARKER_RE.match(follow)
            if not marker_match:
                continue
            marker = marker_match.group(1).lower()
            verdict = verdict_of(resolved)
            if verdict is None:
                issues.append({"row": row_number, "kind": "no-verdict-header", "target": path})
                continue
            if not marker_matches(marker, verdict):
                issues.append({
                    "row": row_number,
                    "kind": "verdict-mismatch",
                    "target": path,
                    "expected": marker.upper(),
                    "found": verdict,
                })
        # Inline corpus references: `34-select-send-unproven-name` and plain "corpus 16".
        inline_seen = set()
        for match in INLINE_CORPUS_RE.finditer(evidence):
            name = match.group(1)
            if not name.endswith(".hy"):
                name += ".hy"
            target = "tests/checker-corpus/" + name
            if target in inline_seen:
                continue
            inline_seen.add(target)
            refs += 1
            if not (root / target).is_file():
                issues.append({"row": row_number, "kind": "missing-inline-corpus", "target": target})
        corpus_dir = root / "tests" / "checker-corpus"
        numbers_seen = set()
        for match in CORPUS_NUM_RE.finditer(evidence):
            number = match.group(1)
            if number in numbers_seen:
                continue
            numbers_seen.add(number)
            refs += 1
            if corpus_dir.is_dir() and not list(corpus_dir.glob("%02d-*.hy" % int(number))):
                issues.append({"row": row_number, "kind": "missing-corpus-number", "target": number})
    return {"ok": not issues, "rows": rows, "links": links, "refs": refs, "issues": issues}


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--root", default=None, help="repository root (default: parent of tools/)")
    parser.add_argument("--json", action="store_true", help="print a JSON report")
    args = parser.parse_args(argv)
    root = Path(args.root).resolve() if args.root else Path(__file__).resolve().parent.parent
    report = check(root)
    if args.json:
        print(json.dumps(report, indent=2, sort_keys=True))
    else:
        print("GUARANTEES evidence: %d row(s), %d link(s), %d inline ref(s), %d issue(s)"
              % (report["rows"], report["links"], report["refs"], len(report["issues"])))
        for issue in report["issues"]:
            print("- row %s: %s %s" % (issue["row"], issue["kind"], issue["target"]))
    return 0 if report["ok"] else 1


if __name__ == "__main__":
    sys.exit(main())
