"""Tests for tools/check_guarantees.py (stdlib only, no network).

The guard exists because GUARANTEES.md's claims table was hand-maintained prose:
nothing bound a row to the rule that implements it or to the corpus fixture that
pins it, so the promise and the rule could drift apart silently. These tests
build throwaway trees and pin every failure mode the guard claims to catch:

  - a row naming a rule that is not in the registry
  - a fixture whose registered verdict contradicts the row that pins it
  - a fixture pinning a diagnostic code the row does not claim
  - a row that names rules but pins no fixture
  - a scoped rule id that appears in no row and is not declared unclaimed
  - an `unclaimed` entry with an empty reason, or naming a rule that is gone
  - an evidence cell citing a corpus path the row does not declare
  - an evidence link that does not resolve
  - the doc's table drifting from the table it is rendered from
  - a doc without the generated-table markers
and the clean case, where the table, the registry, the corpus and the doc agree.
"""
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

SCRIPT = Path(__file__).with_name("check_guarantees.py")
SPEC = importlib.util.spec_from_file_location("check_guarantees", SCRIPT)
MOD = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(MOD)

REGISTRY_REL = "compiler/crates/hy-diag/data/diagnostics.json"
FIXTURE_1 = "tests/checker-corpus/01-shared-map-across-tasks.hy"
FIXTURE_2 = "tests/checker-corpus/15-fanin-per-task-state.hy"

REGISTRY = json.dumps(
    {
        "version": 1,
        "codes": [
            {"code": "HYB-E3001", "rule": "O4", "title": "not sendable at spawn"},
            {"code": "HYB-E3002", "rule": "O4.6", "title": "not sendable at send"},
            {"code": "HYB-E3005", "rule": "O4.9", "title": "fail-closed at a boundary"},
            {"code": "HYB-E8001", "rule": "O6", "title": "managed lint"},
        ],
    },
    ensure_ascii=False,
)

DOC_TEMPLATE = """# GUARANTEES.md — the claims table

Prose about the claim.

## The claims table

| # | Surface | What the checker does | The claim | Evidence |
|---|---------|----------------------|-----------|----------|

## Known limits of this document

- Tier-0 only.
"""


def make_tree(files):
    tmp = tempfile.TemporaryDirectory(prefix="guarantees-")
    root = Path(tmp.name)
    for rel, content in files.items():
        path = root / rel
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(content, encoding="utf-8")
    return tmp, root


def fixture(verdict, diagnostic=None):
    lines = [f"// EXPECTED-VERDICT: {verdict}"]
    if diagnostic:
        lines.append(f"// EXPECTED-DIAGNOSTIC: {diagnostic}")
    lines += ["fn main() {", "}", ""]
    return "\n".join(lines)


def row(row_id=1, rules=None, fixtures=None, evidence="§9.3", **overrides):
    entry = {
        "id": row_id,
        "surface": "surface",
        "does": "what the checker does",
        "claim": "**Guaranteed**: something",
        "strength": "guaranteed",
        "rules": ["HYB-E3001", "HYB-E3005"] if rules is None else rules,
        "fixtures": fixtures
        if fixtures is not None
        else [{"path": FIXTURE_1, "expect": "reject"}],
        "evidence_md": evidence,
    }
    entry.update(overrides)
    return entry


def table(rows, unclaimed=None, scope=("HYB-E3", "HYB-E8")):
    return json.dumps(
        {
            "version": 1,
            "registry": REGISTRY_REL,
            "corpus": "tests/checker-corpus",
            "doc": "GUARANTEES.md",
            "scope": {"families": list(scope)},
            "rows": rows,
            "unclaimed": unclaimed
            if unclaimed is not None
            else {"HYB-E3002": "typing rule, no row yet", "HYB-E8001": "lint, row 8 covers the hatch"},
        },
        ensure_ascii=False,
        indent=2,
    )


def build(rows=None, unclaimed=None, fixtures=None, doc=DOC_TEMPLATE, render=True, registry=REGISTRY):
    """A tree with a consistent table + corpus + doc (rendered unless render=False)."""
    files = {
        REGISTRY_REL: registry,
        "guarantees.json": table(rows if rows is not None else [row()], unclaimed),
        "GUARANTEES.md": doc,
    }
    for rel, content in (fixtures or {FIXTURE_1: fixture("REJECT A1c"), FIXTURE_2: fixture("ACCEPT")}).items():
        files[rel] = content
    tmp, root = make_tree(files)
    if render:
        MOD.write_doc(root, root / "guarantees.json")
    return tmp, root


def problems_of(root):
    return MOD.check(root, root / "guarantees.json")


def joined(problems):
    return "\n".join(problems)


class CleanTree(unittest.TestCase):
    def test_consistent_table_passes(self):
        tmp, root = build()
        self.addCleanup(tmp.cleanup)
        self.assertEqual(problems_of(root), [])

    def test_render_is_idempotent(self):
        tmp, root = build()
        self.addCleanup(tmp.cleanup)
        before = (root / "GUARANTEES.md").read_text(encoding="utf-8")
        MOD.write_doc(root, root / "guarantees.json")
        self.assertEqual((root / "GUARANTEES.md").read_text(encoding="utf-8"), before)
        self.assertEqual(problems_of(root), [])

    def test_adoption_installs_markers(self):
        tmp, root = build(render=False)
        self.addCleanup(tmp.cleanup)
        self.assertIn("carries no generated-table markers", joined(problems_of(root)))
        MOD.write_doc(root, root / "guarantees.json")
        doc = (root / "GUARANTEES.md").read_text(encoding="utf-8")
        self.assertIn(MOD.BEGIN, doc)
        self.assertIn(MOD.END, doc)
        self.assertEqual(problems_of(root), [])


class Rules(unittest.TestCase):
    def test_rule_missing_from_registry_fails(self):
        tmp, root = build(rows=[row(rules=["HYB-E3999"])])
        self.addCleanup(tmp.cleanup)
        text = joined(problems_of(root))
        self.assertIn("HYB-E3999", text)
        self.assertIn("not in the registry", text)

    def test_row_missing_a_required_field_fails(self):
        tmp, root = build(rows=[row(surface="")])
        self.addCleanup(tmp.cleanup)
        self.assertIn("missing `surface`", joined(problems_of(root)))

    def test_duplicate_row_id_fails(self):
        tmp, root = build(rows=[row(1), row(1)])
        self.addCleanup(tmp.cleanup)
        self.assertIn("duplicate row id", joined(problems_of(root)))


class Fixtures(unittest.TestCase):
    def test_registered_verdict_contradicts_the_row(self):
        tmp, root = build(rows=[row(fixtures=[{"path": FIXTURE_1, "expect": "accept"}])])
        self.addCleanup(tmp.cleanup)
        text = joined(problems_of(root))
        self.assertIn("registered REJECT", text)
        self.assertIn("contradicts the row", text)

    def test_missing_fixture_fails(self):
        missing = "tests/checker-corpus/99-does-not-exist.hy"
        tmp, root = build(rows=[row(fixtures=[{"path": missing, "expect": "reject"}])])
        self.addCleanup(tmp.cleanup)
        self.assertIn("does not exist", joined(problems_of(root)))

    def test_fixture_pinning_an_unclaimed_rule_fails(self):
        pinned = {FIXTURE_1: fixture("REJECT FC", "HYB-E3005")}
        tmp, root = build(
            rows=[row(rules=["HYB-E3001"], fixtures=[{"path": FIXTURE_1, "expect": "reject"}])],
            fixtures=pinned,
        )
        self.addCleanup(tmp.cleanup)
        text = joined(problems_of(root))
        self.assertIn("pins HYB-E3005", text)
        self.assertIn("does not claim", text)

    def test_row_with_rules_but_no_fixture_fails(self):
        tmp, root = build(rows=[row(fixtures=[])])
        self.addCleanup(tmp.cleanup)
        self.assertIn("pins no fixture", joined(problems_of(root)))

    def test_evidence_citing_an_undeclared_fixture_fails(self):
        tmp, root = build(rows=[row(evidence=f"corpus [15]({FIXTURE_2})")])
        self.addCleanup(tmp.cleanup)
        self.assertIn("is not declared in `fixtures`", joined(problems_of(root)))

    def test_unresolvable_evidence_link_fails(self):
        tmp, root = build(rows=[row(evidence="[gates/G9.md](gates/G9.md)")])
        self.addCleanup(tmp.cleanup)
        self.assertIn("does not exist", joined(problems_of(root)))

    def test_evidence_link_with_a_section_fragment_resolves(self):
        # A fragment names a place inside the file, so existence is about the file:
        # a section link to a file that is on disk must not read as a miss.
        tmp, root = build(rows=[row(evidence="[scope](SPEC-LANGUAGE.md#scope)")])
        self.addCleanup(tmp.cleanup)
        (root / "SPEC-LANGUAGE.md").write_text("# Scope\n", encoding="utf-8")
        self.assertEqual(problems_of(root), [])

    def test_evidence_link_with_a_line_fragment_resolves(self):
        tmp, root = build(rows=[row(evidence=f"[01]({FIXTURE_1}#L1)")])
        self.addCleanup(tmp.cleanup)
        self.assertEqual(problems_of(root), [])

    def test_missing_file_with_a_fragment_reports_the_whole_target(self):
        # The diagnostic keeps the destination as written — fragment included — so
        # a real miss is still nameable by the reader who has to fix it.
        tmp, root = build(rows=[row(evidence="[gate](gates/G9.md#gate)")])
        self.addCleanup(tmp.cleanup)
        self.assertIn("evidence link `gates/G9.md#gate` does not exist", joined(problems_of(root)))

    def test_evidence_link_with_a_percent_encoded_space_resolves(self):
        # A markdown destination is a URL, so `%20` names a space in the file on
        # disk; a citation to a file whose name needs escaping must still resolve.
        tmp, root = build(rows=[row(evidence="[odd](notes/weird%20name.md)")])
        self.addCleanup(tmp.cleanup)
        (root / "notes").mkdir()
        (root / "notes" / "weird name.md").write_text("# Odd\n", encoding="utf-8")
        self.assertEqual(problems_of(root), [])

    def test_percent_encoded_hash_names_a_literal_hash_in_the_filename(self):
        # Decoding happens after the fragment split, so `%23` stays a literal `#`
        # in the filename instead of being mistaken for an anchor.
        tmp, root = build(rows=[row(evidence="[hash](notes/a%23b.md)")])
        self.addCleanup(tmp.cleanup)
        (root / "notes").mkdir()
        (root / "notes" / "a#b.md").write_text("# Hash\n", encoding="utf-8")
        self.assertEqual(problems_of(root), [])

    def test_missing_percent_encoded_file_is_reported_as_written(self):
        tmp, root = build(rows=[row(evidence="[gone](notes/gone%20file.md)")])
        self.addCleanup(tmp.cleanup)
        self.assertIn("evidence link `notes/gone%20file.md` does not exist", joined(problems_of(root)))

    def test_fixture_without_verdict_line_fails(self):
        tmp, root = build(
            rows=[row()],
            fixtures={FIXTURE_1: "fn main() {\n}\n"},
        )
        self.addCleanup(tmp.cleanup)
        self.assertIn("carries no `// EXPECTED-VERDICT:` line", joined(problems_of(root)))


class Coverage(unittest.TestCase):
    def test_scoped_rule_in_no_row_fails(self):
        # HYB-E3002 is in the scoped family, claimed by no row and not declared unclaimed
        tmp, root = build(unclaimed={"HYB-E8001": "lint, row 8 covers the hatch"})
        self.addCleanup(tmp.cleanup)
        text = joined(problems_of(root))
        self.assertIn("HYB-E3002", text)
        self.assertIn("appears in no row", text)

    def test_unclaimed_with_empty_reason_fails(self):
        tmp, root = build(unclaimed={"HYB-E3002": "   ", "HYB-E8001": "lint"})
        self.addCleanup(tmp.cleanup)
        self.assertIn("empty reason", joined(problems_of(root)))

    def test_unclaimed_naming_a_gone_rule_fails(self):
        tmp, root = build(
            unclaimed={"HYB-E3002": "typing", "HYB-E8001": "lint", "HYB-E9999": "removed rule"}
        )
        self.addCleanup(tmp.cleanup)
        text = joined(problems_of(root))
        self.assertIn("HYB-E9999", text)
        self.assertIn("not in the registry", text)

    def test_rule_outside_the_scope_families_needs_no_row(self):
        registry = json.dumps(
            {
                "version": 1,
                "codes": json.loads(REGISTRY)["codes"]
                + [{"code": "HYB-E1002", "rule": "BUILDABLE", "title": "emitted Go cannot build this"}],
            },
            ensure_ascii=False,
        )
        tmp, root = build(registry=registry)
        self.addCleanup(tmp.cleanup)
        self.assertEqual(problems_of(root), [])


class Doc(unittest.TestCase):
    def test_hand_edited_table_drifts(self):
        tmp, root = build()
        self.addCleanup(tmp.cleanup)
        doc_path = root / "GUARANTEES.md"
        doc_path.write_text(
            doc_path.read_text(encoding="utf-8").replace("**Guaranteed**: something", "**Guaranteed**: something else"),
            encoding="utf-8",
        )
        self.assertIn("differs from the table rendered", joined(problems_of(root)))

    def test_markers_removed_from_the_doc(self):
        tmp, root = build()
        self.addCleanup(tmp.cleanup)
        doc_path = root / "GUARANTEES.md"
        doc_path.write_text(
            doc_path.read_text(encoding="utf-8").replace(MOD.BEGIN, "").replace(MOD.END, ""),
            encoding="utf-8",
        )
        self.assertIn("no generated-table markers", joined(problems_of(root)))

    def test_missing_doc_fails(self):
        tmp, root = build()
        self.addCleanup(tmp.cleanup)
        (root / "GUARANTEES.md").unlink()
        self.assertIn("does not exist", joined(problems_of(root)))


class Misuse(unittest.TestCase):
    def test_invalid_table_json_is_misuse(self):
        tmp, root = make_tree({"guarantees.json": "{not json"})
        self.addCleanup(tmp.cleanup)
        with self.assertRaises(MOD.Misuse):
            MOD.check(root, root / "guarantees.json")

    def test_missing_registry_is_misuse(self):
        tmp, root = make_tree({"guarantees.json": table([row()])})
        self.addCleanup(tmp.cleanup)
        with self.assertRaises(MOD.Misuse):
            MOD.check(root, root / "guarantees.json")

    def test_unclaimed_not_an_object_is_misuse(self):
        tmp, root = build()
        self.addCleanup(tmp.cleanup)
        table_path = root / "guarantees.json"
        data = json.loads(table_path.read_text(encoding="utf-8"))
        data["unclaimed"] = ["HYB-E3002"]
        table_path.write_text(json.dumps(data, ensure_ascii=False, indent=2), encoding="utf-8")
        with self.assertRaises(MOD.Misuse):
            MOD.check(root, table_path)

    def test_main_reports_misuse_with_exit_2(self):
        tmp, root = make_tree({"guarantees.json": "{not json"})
        self.addCleanup(tmp.cleanup)
        self.assertEqual(MOD.main(["--root", str(root)]), 2)

    def test_main_reports_violations_with_exit_1(self):
        tmp, root = build(rows=[row(rules=["HYB-E3999"])])
        self.addCleanup(tmp.cleanup)
        self.assertEqual(MOD.main(["--root", str(root)]), 1)


if __name__ == "__main__":
    unittest.main()


