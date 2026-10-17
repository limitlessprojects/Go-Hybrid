"""Regression tests for the verdict-table guard (offline, stdlib only).

Each case builds a throwaway corpus directory plus a throwaway table, so the
guard is exercised against fixtures rather than the live repository.
"""
import importlib.util
from pathlib import Path
import tempfile
import unittest

SPEC = importlib.util.spec_from_file_location(
    "check_verdict_table", Path(__file__).with_name("check_verdict_table.py"))
guard = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(guard)

STALE_GUARD = """
**Status:** pre-registered before any checker existed.

## Verdict table

| # | program | verdict | rule violated | one-line rationale |
|---|---------|---------|---------------|--------------------|
| 01 | 01-shared-map-across-tasks | REJECT | A1c | a real race |
| 02 | 02-shared-map-managed | ACCEPT-MANAGED | — | the hatch |
| 32 | 32-a3-assign-map-slot-invalidated | REJECT | A3 | re-assignment |  <- stale
| 32 | 32-closure-local-alias-move-reject | REJECT | A1b | closure rebind | <- stale
"""


def write_corpus(root, files):
    """files: {filename: header-line}."""
    corpus = root / "checker-corpus"
    corpus.mkdir(exist_ok=True)
    for name, header in files.items():
        (corpus / name).write_text(header + "\nfn main() {}\n", encoding="utf-8")
    return corpus


def write_table(root, text, name="VERDICTS.md"):
    path = root / name
    path.write_text(text, encoding="utf-8")
    return path


def current_total(total, accept, managed, reject):
    """The one present-tense total header the record must carry."""
    return (f"**Current registered corpus: {total} programs** — ACCEPT "
            f"{accept} · ACCEPT-MANAGED {managed} · REJECT {reject}\n\n")


class PatternTests(unittest.TestCase):
    def test_verdicts_normalize_ignoring_contested(self):
        self.assertEqual(guard.normalize_verdict("REJECT (contested)"), "REJECT")
        self.assertEqual(guard.normalize_verdict("ACCEPT (contested)"), "ACCEPT")
        self.assertEqual(guard.normalize_verdict("ACCEPT-MANAGED"), "ACCEPT-MANAGED")
        self.assertEqual(guard.normalize_verdict("REJECT (contested) "), "REJECT")

    def test_row_and_separator_parsing(self):
        text = (
            "| # | program | verdict | rule |\n"
            "|---|---------|---------|------|\n"
            "| 01 | 01-shared-map-across-tasks | REJECT | A1c | why |\n"
        )
        rows = guard.parse_table(text)
        self.assertEqual(len(rows), 1, rows)
        self.assertEqual(rows[0][0], "01")
        self.assertEqual(rows[0][1], "01-shared-map-across-tasks")
        self.assertEqual(rows[0][2], "REJECT")

    def test_header_requires_the_expected_verdict_line(self):
        self.assertIsNone(guard.HEADER_RE.search("// just a comment\n"))
        match = guard.HEADER_RE.search("// EXPECTED-VERDICT: REJECT A1b\n")
        self.assertIsNotNone(match)
        self.assertEqual(match.group(2), "A1b")


class DriftDetectionTests(unittest.TestCase):
    def build(self, table_text, files):
        temporary = tempfile.TemporaryDirectory(prefix="hybrid-verdicts-")
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        corpus = write_corpus(root, files)
        write_table(root, table_text)
        return root, corpus

    def test_agreeing_table_and_corpus_produce_no_problems(self):
        text = (
            current_total(2, 0, 1, 1) +
            "| # | program | verdict | rule |\n"
            "|---|---------|---------|------|\n"
            "| 01 | 01-shared-map-across-tasks | REJECT | A1c | x |\n"
            "| 02 | 02-shared-map-managed | ACCEPT-MANAGED | — | y |\n"
        )
        root, corpus = self.build(text, {
            "01-shared-map-across-tasks.hy": "// EXPECTED-VERDICT: REJECT A1c",
            "02-shared-map-managed.hy": "// EXPECTED-VERDICT: ACCEPT-MANAGED",
        })
        problems, _, _ = guard.check(text, str(corpus))
        self.assertEqual(problems, [])

    def test_stale_row_number_is_reported_as_a_renumber(self):
        root, corpus = self.build(STALE_GUARD, {
            "01-shared-map-across-tasks.hy": "// EXPECTED-VERDICT: REJECT A1c",
            "02-shared-map-managed.hy": "// EXPECTED-VERDICT: ACCEPT-MANAGED",
            "39-closure-local-alias-move-reject.hy": "// EXPECTED-VERDICT: REJECT A1b",
            "42-a3-assign-map-slot-invalidated.hy": "// EXPECTED-VERDICT: REJECT A3",
        })
        problems, _, _ = guard.check(STALE_GUARD, str(corpus))
        joined = "\n".join(problems)
        self.assertIn("row number 32 appears 2 times", joined)
        self.assertIn("renumber it to 42", joined)
        self.assertIn("renumber it to 39", joined)
        self.assertIn("no VERDICTS.md row names this fixture", joined)

    def test_verdict_disagreement_is_reported(self):
        text = (
            "| # | program | verdict | rule |\n"
            "|---|---------|---------|------|\n"
            "| 01 | 01-shared-map-across-tasks | ACCEPT | — | x |\n"
        )
        root, corpus = self.build(text, {
            "01-shared-map-across-tasks.hy": "// EXPECTED-VERDICT: REJECT A1c",
        })
        problems, _, _ = guard.check(text, str(corpus))
        self.assertTrue(any("table says" in p for p in problems), problems)

    def test_fixture_without_a_header_fails(self):
        text = (
            "| # | program | verdict | rule |\n"
            "|---|---------|---------|------|\n"
            "| 01 | 01-shared-map-across-tasks | REJECT | A1c | x |\n"
        )
        root, corpus = self.build(text, {
            "01-shared-map-across-tasks.hy": "// no header here",
        })
        problems, _, _ = guard.check(text, str(corpus))
        self.assertTrue(any("no `// EXPECTED-VERDICT:` line" in p for p in problems),
                        problems)

    def test_fixture_with_no_row_is_reported(self):
        text = (
            "| # | program | verdict | rule |\n"
            "|---|---------|---------|------|\n"
            "| 01 | 01-shared-map-across-tasks | REJECT | A1c | x |\n"
        )
        root, corpus = self.build(text, {
            "01-shared-map-across-tasks.hy": "// EXPECTED-VERDICT: REJECT A1c",
            "02-shared-map-managed.hy": "// EXPECTED-VERDICT: ACCEPT-MANAGED",
        })
        problems, _, _ = guard.check(text, str(corpus))
        self.assertTrue(
            any("no VERDICTS.md row names this fixture" in p for p in problems),
            problems)


class DistributionCaptionTests(unittest.TestCase):
    """The running `**Distribution: … — N programs.**` totals."""

    ROWS = (
        "| # | program | verdict | rule |\n"
        "|---|---------|---------|------|\n"
        "| 01 | 01-a | REJECT | A1c | x |\n"
        "| 02 | 02-b | ACCEPT | — | y |\n"
        "| 03 | 03-c | ACCEPT-MANAGED | — | z |\n"
    )
    FILES = {
        "01-a.hy": "// EXPECTED-VERDICT: REJECT A1c",
        "02-b.hy": "// EXPECTED-VERDICT: ACCEPT",
        "03-c.hy": "// EXPECTED-VERDICT: ACCEPT-MANAGED",
    }

    def build(self, caption):
        temporary = tempfile.TemporaryDirectory(prefix="hybrid-dist-")
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        corpus = write_corpus(root, self.FILES)
        return corpus, current_total(3, 1, 1, 1) + self.ROWS + "\n" + caption + "\n"

    def test_caption_matching_its_own_arithmetic_and_the_corpus_is_clean(self):
        corpus, text = self.build(
            "**Distribution: ACCEPT 1 · ACCEPT-MANAGED 1 · REJECT 1 — 3 programs.**")
        problems, _, _ = guard.check(text, str(corpus))
        self.assertEqual(problems, [])

    def test_caption_that_does_not_add_up_is_reported(self):
        corpus, text = self.build(
            "**Distribution: ACCEPT 0 · ACCEPT-MANAGED 1 · REJECT 1 — 3 programs.**")
        problems, _, _ = guard.check(text, str(corpus))
        self.assertTrue(
            any("does not add up" in p for p in problems), problems)

    def test_caption_disagreeing_with_the_corpus_is_reported(self):
        corpus, text = self.build(
            "**Distribution: ACCEPT 2 · ACCEPT-MANAGED 1 · REJECT 0 — 3 programs.**")
        problems, _, _ = guard.check(text, str(corpus))
        self.assertTrue(
            any("disagrees with the corpus" in p for p in problems), problems)

    def test_caption_naming_more_programs_than_exist_is_reported(self):
        corpus, text = self.build(
            "**Distribution: ACCEPT 3 · ACCEPT-MANAGED 3 · REJECT 3 — 9 programs.**")
        problems, _, _ = guard.check(text, str(corpus))
        self.assertTrue(
            any("disagrees with the corpus" in p for p in problems), problems)

    def test_parse_distributions_reads_all_three_counts(self):
        found = guard.parse_distributions(
            "prose\n"
            "**Distribution: ACCEPT 12 · ACCEPT-MANAGED 2 · REJECT 31 — 45 "
            "programs.**\ntail\n")
        self.assertEqual(len(found), 1, found)
        line, _, stated, total = found[0]
        self.assertEqual(line, 2)
        self.assertEqual(stated, {"ACCEPT": 12, "ACCEPT-MANAGED": 2, "REJECT": 31})
        self.assertEqual(total, 45)

    def test_parse_distributions_flags_a_caption_without_numbers(self):
        found = guard.parse_distributions("**Distribution: some prose.**\n")
        self.assertEqual(len(found), 1, found)
        _, _, stated, total = found[0]
        self.assertIsNone(stated)
        self.assertIsNone(total)


class CurrentTotalTests(unittest.TestCase):
    """The one `**Current registered corpus: N programs**` header."""

    ROWS = (
        "| # | program | verdict | rule |\n"
        "|---|---------|---------|------|\n"
        "| 01 | 01-a | REJECT | A1c | x |\n"
        "| 02 | 02-b | ACCEPT | — | y |\n"
    )
    FILES = {
        "01-a.hy": "// EXPECTED-VERDICT: REJECT A1c",
        "02-b.hy": "// EXPECTED-VERDICT: ACCEPT",
    }

    def problems_for(self, header):
        temporary = tempfile.TemporaryDirectory(prefix="hybrid-total-")
        self.addCleanup(temporary.cleanup)
        root = Path(temporary.name)
        corpus = write_corpus(root, self.FILES)
        text = header + self.ROWS
        problems, _, _ = guard.check(text, str(corpus))
        return [p for p in problems if "Current registered corpus" in p]

    def test_matching_header_is_clean(self):
        self.assertEqual(self.problems_for(current_total(2, 1, 0, 1)), [])

    def test_header_that_does_not_add_up_is_reported(self):
        problems = self.problems_for(current_total(2, 1, 0, 0))
        self.assertTrue(any("does not add up" in p for p in problems), problems)

    def test_header_disagreeing_with_the_corpus_is_reported(self):
        problems = self.problems_for(current_total(3, 2, 0, 1))
        self.assertTrue(any("corpus holds 2" in p for p in problems), problems)

    def test_header_wrapped_across_lines_is_still_read(self):
        wrapped = ("**Current registered corpus: 2 programs** — ACCEPT 1 ·\n"
                   "ACCEPT-MANAGED 0 · REJECT 1\n\n")
        self.assertEqual(self.problems_for(wrapped), [])

    def test_missing_header_is_reported(self):
        problems = self.problems_for("")
        self.assertTrue(any("no `**Current registered corpus" in p
                            for p in problems), problems)

    def test_the_stale_28_programs_wording_is_caught_when_reintroduced(self):
        # The exact old shape that went stale on `main`:
        #   "corpus total. Current registered corpus: **28 programs: ACCEPT 10 ·"
        #   "ACCEPT-MANAGED 2 · REJECT 16**."
        # Reintroducing it verbatim must be reported, not accepted as the
        # current total.
        stale = ("corpus total. Current registered corpus: **28 programs: "
                 "ACCEPT 10 · ACCEPT-MANAGED 2 · REJECT 16**.\n\n")
        problems = self.problems_for(stale)
        self.assertTrue(any("corpus holds 2" in p for p in problems), problems)

    def test_a_stale_header_above_the_real_one_is_not_shadowed(self):
        stale = ("Current registered corpus: **28 programs: ACCEPT 10 · "
                 "ACCEPT-MANAGED 2 · REJECT 16**.\n\n")
        problems = self.problems_for(stale + current_total(2, 1, 0, 1))
        self.assertTrue(any("corpus holds 2" in p for p in problems), problems)


class GuardMisuseTests(unittest.TestCase):
    def run_main(self, table=True, corpus=True, table_text=None):
        with tempfile.TemporaryDirectory(prefix="hybrid-vt-") as temporary:
            root = Path(temporary)
            table_path = root / "VERDICTS.md"
            corpus_dir = root / "checker-corpus"
            if corpus:
                write_corpus(root, {"01-x.hy": "// EXPECTED-VERDICT: REJECT A1c"})
            if table:
                write_table(root, table_text or (
                    current_total(1, 0, 0, 1) +
                    "| # | program | verdict | rule |\n"
                    "|---|---------|---------|------|\n"
                    "| 01 | 01-x | REJECT | A1c | x |\n"
                    "**Distribution: ACCEPT 0 · ACCEPT-MANAGED 0 · REJECT 1 — 1 "
                    "programs.**\n"))
            original = guard.sys.argv
            guard.sys.argv = ["check_verdict_table.py",
                              "--table", str(table_path),
                              "--corpus", str(corpus_dir)]
            try:
                import contextlib
                import io
                with contextlib.redirect_stdout(io.StringIO()), \
                        contextlib.redirect_stderr(io.StringIO()):
                    return guard.main()
            finally:
                guard.sys.argv = original

    def test_missing_table_fails_closed(self):
        self.assertEqual(self.run_main(table=False), 2)

    def test_missing_corpus_fails_closed(self):
        self.assertEqual(self.run_main(corpus=False), 2)

    def test_table_with_no_rows_fails_closed(self):
        self.assertEqual(self.run_main(table_text="# just prose\n"), 2)

    def test_table_with_no_distribution_caption_fails_closed(self):
        no_caption = (
            current_total(1, 0, 0, 1) +
            "| # | program | verdict | rule |\n"
            "|---|---------|---------|------|\n"
            "| 01 | 01-x | REJECT | A1c | x |\n")
        self.assertEqual(self.run_main(table_text=no_caption), 2)

    def test_table_with_no_current_total_header_fails_closed(self):
        no_total = (
            "| # | program | verdict | rule |\n"
            "|---|---------|---------|------|\n"
            "| 01 | 01-x | REJECT | A1c | x |\n"
            "**Distribution: ACCEPT 0 · ACCEPT-MANAGED 0 · REJECT 1 — 1 "
            "programs.**\n")
        self.assertEqual(self.run_main(table_text=no_total), 2)

    def test_agreeing_inputs_exit_zero(self):
        self.assertEqual(self.run_main(), 0)

    def test_drift_exits_one(self):
        drift = (
            current_total(1, 1, 0, 0) +
            "| # | program | verdict | rule |\n"
            "|---|---------|---------|------|\n"
            "| 01 | 01-x | ACCEPT | — | x |\n"
            "**Distribution: ACCEPT 1 · ACCEPT-MANAGED 0 · REJECT 0 — 1 "
            "programs.**\n"
        )
        self.assertEqual(self.run_main(table_text=drift), 1)


if __name__ == "__main__":
    unittest.main()
