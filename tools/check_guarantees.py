#!/usr/bin/env python3
"""check_guarantees.py — GUARANTEES.md is generated from a table, and the table is checked (issue #710).

GUARANTEES.md is the document users are told to trust: it promises what the
checker does and where it declines to claim anything. Its claims table was
hand-maintained prose, so nothing mechanically bound a row to the rule that
implements it or to the corpus fixture that pins it — the promise and the rule
could drift apart silently, in both directions.

This guard makes the table data. `guarantees.json` holds one row per claim:

    id, surface, does, claim, strength, rules[], fixtures[{path, expect}], evidence_md

`GUARANTEES.md`'s table is RENDERED from that file (`--render`), and every check
below runs against the repository as it is on disk:

  rules       every rule id a row names must exist in the diagnostics registry
              (compiler/crates/hy-diag/data/diagnostics.json — append-only, so a
              missing id means the row is stale)
  fixtures    every fixture must exist and its case-sensitive
              `// EXPECTED-VERDICT:` line must agree with the row's `expect`
              (accept / accept-managed / reject); when a fixture additionally
              pins `// EXPECTED-DIAGNOSTIC: HYB-Exxxx`, that code must be one of
              the row's rules — a fixture pinning a rule the row does not claim
              is exactly the drift this guard exists for
  prose       a corpus path cited in the row's evidence cell must be declared in
              the row's `fixtures` (prose cannot cite an unvalidated fixture)
  no-fixture  a row that names rules must pin at least one fixture: a claimed
              rule with no fixture is an unbacked promise
  coverage    every registry code in a scoped family (default HYB-E3 ownership &
              sendability + HYB-E8 lint) must be claimed by some row or listed in
              `unclaimed` with a reason — a new rule landing in no row fails
  unclaimed   every `unclaimed` key must still exist in the registry
  links       relative markdown links in an evidence cell must resolve
  doc         the rendered table must equal the table in GUARANTEES.md, so the
              document cannot be edited by hand out of step with the table

Usage:
    python3 tools/check_guarantees.py                  # verify table + doc
    python3 tools/check_guarantees.py --render         # rewrite the doc's table block
    python3 tools/check_guarantees.py --table F --doc D --root DIR   # alternate tree

Exit codes: 0 = the table, the registry, the corpus and the doc agree;
1 = a violation (each one is named with the row it belongs to); 2 = guard misuse.
"""


from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent

DEFAULT_TABLE = Path("guarantees.json")
BEGIN = "<!-- BEGIN GENERATED CLAIMS TABLE (tools/check_guarantees.py --render) -->"
END = "<!-- END GENERATED CLAIMS TABLE -->"
HEADER = "| # | Surface | What the checker does | The claim | Evidence |"
SEPARATOR = "|---|---------|----------------------|-----------|----------|"

VERDICT_RE = re.compile(
    r"^//[ \t]*EXPECTED-VERDICT:[ \t]*(ACCEPT-MANAGED|ACCEPT|REJECT)(?![-\w])", re.M
)
DIAGNOSTIC_RE = re.compile(r"^//[ \t]*EXPECTED-DIAGNOSTIC:[ \t]*(HYB-E\d+)", re.M)
LINK_RE = re.compile(r"\]\(([^)\s]+)\)")
CORPUS_PATH_RE = re.compile(r"tests/checker-corpus/[A-Za-z0-9._-]+\.hy")

EXPECT_DIRECTION = {
    "accept": "ACCEPT",
    "accept-managed": "ACCEPT-MANAGED",
    "reject": "REJECT",
}


class Misuse(Exception):
    """Guard misuse (exit 2), not a repository violation."""


def load_table(path: Path) -> dict:
    try:
        raw = path.read_text(encoding="utf-8")
    except OSError as exc:
        raise Misuse(f"cannot read the table {path}: {exc}") from exc
    try:
        table = json.loads(raw)
    except json.JSONDecodeError as exc:
        raise Misuse(f"{path} is not valid JSON: {exc}") from exc
    if not isinstance(table, dict) or not isinstance(table.get("rows"), list):
        raise Misuse(f"{path} must be an object with a `rows` list")
    return table


def registry_codes(path: Path) -> dict[str, dict]:
    try:
        data = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError) as exc:
        raise Misuse(f"cannot read the diagnostics registry {path}: {exc}") from exc
    codes: dict[str, dict] = {}
    for entry in data.get("codes", []):
        code = entry.get("code")
        if isinstance(code, str):
            codes[code] = entry
    if not codes:
        raise Misuse(f"{path} lists no diagnostic codes")
    return codes


def fixture_verdict(path: Path) -> tuple[str | None, str | None]:
    """(registered verdict, pinned diagnostic code) — case-sensitive, like corpus_harness."""
    text = path.read_text(encoding="utf-8")
    verdict = VERDICT_RE.search(text)
    pinned = DIAGNOSTIC_RE.search(text)
    return (verdict.group(1) if verdict else None, pinned.group(1) if pinned else None)


def render_table(table: dict) -> str:
    lines = [BEGIN, HEADER, SEPARATOR]
    for row in table["rows"]:
        lines.append(
            "| {id} | {surface} | {does} | {claim} | {evidence} |".format(
                id=row["id"],
                surface=row["surface"],
                does=row["does"],
                claim=row["claim"],
                evidence=row["evidence_md"],
            )
        )
    lines.append(END)
    return "\n".join(lines) + "\n"


def check(root: Path, table_path: Path, doc_override: str | None = None) -> list[str]:
    table = load_table(table_path)
    problems: list[str] = []

    registry_path = root / table.get("registry", "compiler/crates/hy-diag/data/diagnostics.json")
    corpus_rel = table.get("corpus", "tests/checker-corpus")
    doc_rel = doc_override or table.get("doc", "GUARANTEES.md")
    codes = registry_codes(registry_path)

    families = tuple(table.get("scope", {}).get("families", []))
    unclaimed = table.get("unclaimed", {})
    if not isinstance(unclaimed, dict):
        raise Misuse("`unclaimed` must be an object of code -> reason")

    claimed_codes: set[str] = set()
    seen_ids: set[int] = set()

    for row in table["rows"]:
        rid = row.get("id")
        where = f"row {rid}"
        for field in ("surface", "does", "claim", "strength", "evidence_md"):
            if not row.get(field):
                problems.append(f"{where}: missing `{field}`")
        if rid in seen_ids:
            problems.append(f"{where}: duplicate row id")
        seen_ids.add(rid)

        rules = row.get("rules", [])
        fixtures = row.get("fixtures", [])
        if not isinstance(rules, list) or not isinstance(fixtures, list):
            problems.append(f"{where}: `rules` and `fixtures` must be lists")
            continue

        for code in rules:
            if code not in codes:
                problems.append(
                    f"{where}: names the rule {code}, which is not in the registry "
                    f"({registry_path.name}) — the row is stale"
                )
            claimed_codes.add(code)

        if rules and not fixtures:
            problems.append(f"{where}: names rules {', '.join(rules)} but pins no fixture")

        evidence_md = row.get("evidence_md", "")
        for entry in fixtures:
            path_rel = entry.get("path", "")
            expect = entry.get("expect", "")
            if expect not in EXPECT_DIRECTION:
                problems.append(f"{where}: fixture {path_rel} has unknown expect `{expect}`")
                continue
            fixture = root / path_rel
            if not fixture.is_file():
                problems.append(f"{where}: fixture {path_rel} does not exist")
                continue
            if not str(path_rel).startswith(corpus_rel):
                problems.append(f"{where}: fixture {path_rel} is outside {corpus_rel}")
            verdict, pinned = fixture_verdict(fixture)
            if verdict is None:
                problems.append(f"{where}: fixture {path_rel} carries no `// EXPECTED-VERDICT:` line")
            elif verdict != EXPECT_DIRECTION[expect]:
                problems.append(
                    f"{where}: fixture {path_rel} is registered {verdict}, but the row pins it as "
                    f"`{expect}` — the fixture contradicts the row"
                )
            if pinned and pinned not in rules:
                problems.append(
                    f"{where}: fixture {path_rel} pins {pinned}, which the row does not claim "
                    f"({', '.join(rules) or 'no rules'})"
                )

        declared = {e.get("path") for e in fixtures}
        for cited in sorted(set(CORPUS_PATH_RE.findall(evidence_md))):
            if cited not in declared:
                problems.append(
                    f"{where}: the evidence cell cites {cited}, which is not declared in `fixtures`"
                )

        for target in LINK_RE.findall(evidence_md):
            if target.startswith(("http://", "https://", "#", "mailto:")):
                continue
            if not (root / target).exists():
                problems.append(f"{where}: evidence link `{target}` does not exist")

    for code, entry in sorted(codes.items()):
        if families and not code.startswith(tuple(families)):
            continue
        if code in claimed_codes:
            continue
        if code in unclaimed:
            if not str(unclaimed[code]).strip():
                problems.append(f"coverage: {code} is listed in `unclaimed` with an empty reason")
            continue
        problems.append(
            f"coverage: {code} ({entry.get('title', '')}) is in a scoped family and appears in no "
            f"row — claim it or list it in `unclaimed` with a reason"
        )
    for code in sorted(unclaimed):
        if code not in codes:
            problems.append(f"coverage: `unclaimed` names {code}, which is not in the registry")

    rendered = render_table(table)
    doc_path = root / doc_rel
    if not doc_path.is_file():
        problems.append(f"doc: {doc_rel} does not exist")
    else:
        doc = doc_path.read_text(encoding="utf-8")
        if BEGIN in doc and END in doc:
            current = doc[doc.index(BEGIN) : doc.index(END) + len(END)] + "\n"
            if current != rendered:
                problems.append(
                    f"doc: the table in {doc_rel} differs from the table rendered from "
                    f"{table_path.name} — run `python3 tools/check_guarantees.py --render`"
                )
        else:
            problems.append(
                f"doc: {doc_rel} carries no generated-table markers — run --render once to adopt them"
            )
    return problems


def write_doc(root: Path, table_path: Path, doc_override: str | None = None) -> None:
    table = load_table(table_path)
    doc_path = root / (doc_override or table.get("doc", "GUARANTEES.md"))
    rendered = render_table(table)
    if not doc_path.is_file():
        raise Misuse(f"doc: {doc_path} does not exist")
    doc = doc_path.read_text(encoding="utf-8")
    if BEGIN in doc and END in doc:
        updated = doc[: doc.index(BEGIN)] + rendered + doc[doc.index(END) + len(END) + 1 :]
    else:
        # first adoption: replace the contiguous table block that follows the header row
        lines = doc.splitlines(keepends=True)
        start = next((i for i, line in enumerate(lines) if line.strip() == HEADER), None)
        if start is None:
            raise Misuse(f"doc: no claims table found in {doc_path} to replace")
        end = start
        while end < len(lines) and lines[end].lstrip().startswith("|"):
            end += 1
        updated = "".join(lines[:start]) + rendered + "".join(lines[end:])
    doc_path.write_text(updated, encoding="utf-8")
    print(f"rendered {len(table['rows'])} claim rows into {doc_path}")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(
        description="Check (or render) the GUARANTEES.md claims table."
    )
    parser.add_argument("--root", default=str(REPO_ROOT), help="repository root (default: this repo)")
    parser.add_argument("--table", default=None, help=f"table path (default: {DEFAULT_TABLE})")
    parser.add_argument("--doc", default=None, help="doc path override (default: from the table)")
    parser.add_argument("--render", action="store_true", help="rewrite the doc's table from the table")
    args = parser.parse_args(argv)

    root = Path(args.root).resolve()
    table_path = Path(args.table) if args.table else DEFAULT_TABLE
    if not table_path.is_absolute():
        table_path = root / table_path

    try:
        if args.render:
            write_doc(root, table_path, args.doc)
        problems = check(root, table_path, args.doc)
    except Misuse as exc:
        print(f"guard misuse: {exc}", file=sys.stderr)
        return 2

    if problems:
        print("GUARANTEES table check FAILED:")
        for problem in problems:
            print(f"  - {problem}")
        return 1
    print("guarantees OK (table, registry, corpus and doc agree)")
    return 0


if __name__ == "__main__":
    sys.exit(main())
