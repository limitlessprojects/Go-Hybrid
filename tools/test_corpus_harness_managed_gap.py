"""Tests for the #3251 managed-report gap in tools/corpus_harness.py.

--report-managed asks the checker for a managed report. A parse failure
legitimately omits it (the counters are computed after parsing). Any other
response that omits it is a measurement gap, and the harness used to fold
that gap into the aggregate as a measured zero: "0 block(s), 0/0 sites",
exit 0. These tests pin the distinction.

Stub checkers only, no real binary, no network.
"""
import json
import os
import stat
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HARNESS = Path(__file__).with_name("corpus_harness.py")


def write_stub(root, name, body):
    path = Path(root) / name
    path.write_text(body)
    path.chmod(path.stat().st_mode | stat.S_IEXEC)
    return path


def run_harness(binary, corpus, *extra):
    return subprocess.run(
        [sys.executable, str(HARNESS), "--bin", str(binary), "--corpus", str(corpus)]
        + list(extra),
        capture_output=True,
        text=True,
        timeout=120,
    )


ACCEPT_NO_MANAGED = (
    "#!/bin/sh\n"
    "printf '%s\\n' '{\"summary\":{\"verdict\":\"accept\",\"errors\":0,"
    "\"warnings\":0},\"diagnostics\":[]}'\n"
)

ACCEPT_WITH_MANAGED = (
    "#!/bin/sh\n"
    "printf '%s\\n' '{\"summary\":{\"verdict\":\"accept\",\"errors\":0,"
    "\"warnings\":0,\"managed\":{\"managed_blocks\":0,"
    "\"sites_via_managed\":0,\"total_sites\":0}},\"diagnostics\":[]}'\n"
)

PARSE_FAIL_NO_MANAGED = (
    "#!/bin/sh\n"
    "printf '%s\\n' '{\"summary\":{\"verdict\":\"reject\",\"errors\":1,"
    "\"warnings\":0},\"diagnostics\":[{\"code\":\"HYB-E0002\","
    "\"severity\":\"error\",\"message\":\"parse error\","
    "\"tier0_rule\":\"HYB-E0002\","
    "\"span\":{\"start\":{\"line\":1,\"col\":1},\"end\":{\"line\":1,"
    "\"col\":1}}}]}'\n"
    "exit 1\n"
)


class ManagedReportGap(unittest.TestCase):
    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory(prefix="hy3251-")
        self.root = Path(self._tmp.name)
        self.corpus = self.root / "corpus"
        self.corpus.mkdir()

    def tearDown(self):
        self._tmp.cleanup()

    def _case(self, fname, expected):
        (self.corpus / fname).write_text("// EXPECTED-VERDICT: %s\nfn main() {}\n" % expected)

    def test_accepted_response_without_report_is_a_harness_error(self):
        self._case("01-plain.hy", "ACCEPT")
        binary = write_stub(self.root, "hyb", ACCEPT_NO_MANAGED)
        proc = run_harness(binary, self.corpus, "--report-managed")
        self.assertEqual(proc.returncode, 1, proc.stdout + proc.stderr)
        self.assertIn("harness-error", proc.stdout)
        self.assertIn("omitted the requested managed report", proc.stdout)

    def test_gap_marks_the_aggregate_unavailable_not_zero(self):
        self._case("01-plain.hy", "ACCEPT")
        binary = write_stub(self.root, "hyb", ACCEPT_NO_MANAGED)
        proc = run_harness(binary, self.corpus, "--report-managed")
        self.assertIn("managed aggregate: unavailable", proc.stdout)
        # The exact regression: a gap must never print as a measured zero.
        self.assertNotIn("managed aggregate: 0 block(s), 0/0 sites", proc.stdout)

    def test_present_report_still_aggregates_numerically(self):
        self._case("01-plain.hy", "ACCEPT")
        binary = write_stub(self.root, "hyb", ACCEPT_WITH_MANAGED)
        proc = run_harness(binary, self.corpus, "--report-managed")
        self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
        self.assertIn("managed aggregate: 0 block(s), 0/0 sites", proc.stdout)

    def test_parse_failure_without_report_is_not_a_gap(self):
        self._case("01-parse.hy", "REJECT HYB-E0002")
        binary = write_stub(self.root, "hyb", PARSE_FAIL_NO_MANAGED)
        proc = run_harness(binary, self.corpus, "--report-managed")
        self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)
        self.assertNotIn("omitted the requested managed report", proc.stdout)
        self.assertIn("managed aggregate", proc.stdout)

    def test_without_report_managed_the_gap_is_irrelevant(self):
        self._case("01-plain.hy", "ACCEPT")
        binary = write_stub(self.root, "hyb", ACCEPT_NO_MANAGED)
        proc = run_harness(binary, self.corpus)
        self.assertEqual(proc.returncode, 0, proc.stdout + proc.stderr)


if __name__ == "__main__":
    unittest.main()
