"""CLI regressions for optional exact diagnostic metadata (stdlib only)."""

import contextlib
import io
import json
from pathlib import Path
import os
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

import corpus_harness

HARNESS = Path(__file__).with_name("corpus_harness.py")


class ExactDiagnosticCliTests(unittest.TestCase):
    def run_harness(self, header, code="HYB-E3002", verdict="REJECT A1c"):
        # A tiny deterministic checker exercises the complete harness CLI:
        # fixture metadata -> subprocess JSON -> rule/code comparison -> exit.
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            corpus = root / "corpus"
            corpus.mkdir()
            (corpus / "01-example.hy").write_text(
                f"// EXPECTED-VERDICT: {verdict}\n{header}\nfn main() {{}}\n",
                encoding="utf-8",
            )
            payload = {
                "summary": {"verdict": "reject"},
                "diagnostics": [{"severity": "error", "tier0_rule": "A1c", "code": code}],
            }
            checker = root / "hyb"
            checker.write_text(
                f"#!{sys.executable}\nimport json\n"
                f"print(json.dumps({payload!r}))\nraise SystemExit(1)\n",
                encoding="utf-8",
            )
            checker.chmod(0o700)
            return subprocess.run(
                [sys.executable, str(HARNESS), "--bin", str(checker), "--corpus", str(corpus)],
                capture_output=True, text=True, timeout=10,
            )

    def test_wrong_code_with_same_rule_fails(self):
        run = self.run_harness("// EXPECTED-DIAGNOSTIC: HYB-E3002", code="HYB-E3001")
        self.assertEqual(run.returncode, 1, run.stdout + run.stderr)
        self.assertIn("expected first diagnostic HYB-E3002, got HYB-E3001", run.stdout)
        self.assertIn("verdict agreement: 0/1", run.stdout)

    def test_exact_code_match_passes(self):
        run = self.run_harness("// EXPECTED-DIAGNOSTIC: HYB-E3002")
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
        self.assertIn("verdict agreement: 1/1", run.stdout)

    def test_old_fixture_without_header_keeps_rule_only_contract(self):
        run = self.run_harness("", code="HYB-E3001")
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
        self.assertIn("verdict agreement: 1/1", run.stdout)

    def test_malformed_header_fails_closed(self):
        for header in (
            "// EXPECTED-DIAGNOSTIC: not-a-code",
            "// EXPECTED-DIAGNOSTIC HYB-E3002",
            "// EXPECTED-DIAGNOSTIC:",
        ):
            with self.subTest(header=header):
                run = self.run_harness(header)
                self.assertEqual(run.returncode, 2, run.stdout + run.stderr)
                self.assertIn("malformed EXPECTED-DIAGNOSTIC", run.stderr)

    def test_duplicate_headers_fail_closed(self):
        run = self.run_harness(
            "// EXPECTED-DIAGNOSTIC: HYB-E3002\n// EXPECTED-DIAGNOSTIC: HYB-E3002"
        )
        self.assertEqual(run.returncode, 2, run.stdout + run.stderr)
        self.assertIn("multiple EXPECTED-DIAGNOSTIC", run.stderr)

    def test_accept_fixture_cannot_declare_error_code(self):
        run = self.run_harness("// EXPECTED-DIAGNOSTIC: HYB-E3002", verdict="ACCEPT")
        self.assertEqual(run.returncode, 2, run.stdout + run.stderr)
        self.assertIn("without REJECT", run.stderr)

    def test_duplicate_verdict_headers_do_not_silently_pass(self):
        # The checker agrees with the first header. The second makes the
        # fixture ambiguous and must not be silently ignored by the harness.
        run = self.run_harness("// EXPECTED-VERDICT: ACCEPT")
        self.assertEqual(run.returncode, 2, run.stdout + run.stderr)
        self.assertIn("multiple EXPECTED-VERDICT", run.stderr)

    def test_malformed_second_verdict_header_does_not_silently_pass(self):
        run = self.run_harness("// EXPECTED-VERDICT: ACCEPT-MANGLED")
        self.assertEqual(run.returncode, 2, run.stdout + run.stderr)
        self.assertIn("multiple EXPECTED-VERDICT", run.stderr)

    def test_malformed_only_verdict_header_is_not_treated_as_accept(self):
        run = self.run_harness("", verdict="ACCEPT-MANGLED")
        self.assertEqual(run.returncode, 2, run.stdout + run.stderr)
        self.assertIn("malformed EXPECTED-VERDICT", run.stderr)

    def test_accept_header_with_unexpected_rule_fails_closed(self):
        run = self.run_harness("", verdict="ACCEPT A1c")
        self.assertEqual(run.returncode, 2, run.stdout + run.stderr)
        self.assertIn("unexpected rule after ACCEPT", run.stderr)

    def test_reject_header_requires_rule_id(self):
        run = self.run_harness("", verdict="REJECT")
        self.assertEqual(run.returncode, 2, run.stdout + run.stderr)
        self.assertIn("REJECT without a rule id", run.stderr)


class CheckerEnvelopeCliTests(unittest.TestCase):
    def run_payload(self, payload, expected="ACCEPT", status=0):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            corpus = root / "corpus"
            corpus.mkdir()
            (corpus / "01-example.hy").write_text(
                f"// EXPECTED-VERDICT: {expected}\nfn main() {{}}\n", encoding="utf-8"
            )
            checker = root / "hyb"
            checker.write_text(
                f"#!{sys.executable}\nimport json\n"
                f"print(json.dumps({payload!r}))\nraise SystemExit({status})\n",
                encoding="utf-8",
            )
            checker.chmod(0o700)
            return subprocess.run(
                [sys.executable, str(HARNESS), "--bin", str(checker), "--corpus", str(corpus)],
                capture_output=True, text=True, timeout=10,
            )

    def assert_invalid(self, payload, expected="ACCEPT", status=0):
        run = self.run_payload(payload, expected, status)
        self.assertEqual(run.returncode, 1, run.stdout + run.stderr)
        self.assertIn("harness-error", run.stdout)
        self.assertIn("verdict agreement: 0/1", run.stdout)
        self.assertNotIn("Traceback", run.stderr)

    def test_matching_counts_accept(self):
        payload = {"summary": {"verdict": "accept", "errors": 0, "warnings": 1},
                   "diagnostics": [{"severity": "warning", "code": "HYB-E8001"}]}
        run = self.run_payload(payload, expected="ACCEPT-MANAGED")
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)

    def test_matching_counts_reject(self):
        payload = {"summary": {"verdict": "reject", "errors": 1, "warnings": 0},
                   "diagnostics": [{"severity": "error", "tier0_rule": "A1c"}]}
        run = self.run_payload(payload, expected="REJECT A1c", status=1)
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)

    def test_omitted_counts_keep_existing_contract(self):
        run = self.run_payload({"summary": {"verdict": "accept"}, "diagnostics": []})
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)

    def test_reported_errors_must_match_diagnostics(self):
        self.assert_invalid({"summary": {"verdict": "accept", "errors": 1},
                             "diagnostics": []})
        self.assert_invalid({"summary": {"verdict": "reject", "errors": 0},
                             "diagnostics": [{"severity": "error", "tier0_rule": "A1c"}]},
                            expected="REJECT A1c", status=1)

    def test_reported_warnings_must_match_diagnostics(self):
        self.assert_invalid({"summary": {"verdict": "accept", "warnings": 1},
                             "diagnostics": []})

    def test_counts_require_nonnegative_integers(self):
        for count in (False, True, None, "0", 0.0, -1):
            with self.subTest(count=count):
                self.assert_invalid({"summary": {"verdict": "accept", "errors": count},
                                     "diagnostics": []})

    def test_invalid_envelope_shapes_fail_without_traceback(self):
        for payload in (
            None, [], {"summary": None, "diagnostics": []},
            {"summary": [], "diagnostics": []},
            {"summary": {"verdict": "accept"}, "diagnostics": None},
            {"summary": {"verdict": "accept"}, "diagnostics": {}},
            {"summary": {"verdict": "accept"}, "diagnostics": [None]},
            {"summary": {"verdict": "accept"}, "diagnostics": ["error"]},
        ):
            with self.subTest(payload=payload):
                self.assert_invalid(payload)

    def test_extra_fields_do_not_change_acceptance(self):
        run = self.run_payload({"summary": {"verdict": "accept", "errors": 0,
                                            "warnings": 0, "future": "kept"},
                                "diagnostics": [], "version": 1, "future": {}})
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)


class ManagedReportCliTests(unittest.TestCase):
    """Exercise managed aggregation through the actual harness subprocess."""

    def accept_payload(self, managed):
        return {
            "summary": {"verdict": "accept", "errors": 0, "warnings": 0,
                        "managed": managed},
            "diagnostics": [],
        }

    def run_cases(self, cases, report_managed=True):
        # Each (expected verdict, JSON payload, checker exit) is a separate
        # corpus file. A deterministic checker keeps this independent of the
        # Go/Rust toolchains while exercising real CLI parsing and reporting.
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            corpus = root / "corpus"
            corpus.mkdir()
            responses = {}
            for index, (expected, payload, status) in enumerate(cases, 1):
                filename = f"{index:02}-example.hy"
                (corpus / filename).write_text(
                    f"// EXPECTED-VERDICT: {expected}\nfn main() {{}}\n",
                    encoding="utf-8",
                )
                responses[filename] = (payload, status)
            checker = root / "hyb"
            checker.write_text(
                f"#!{sys.executable}\nimport json\nimport sys\nfrom pathlib import Path\n"
                f"responses = {responses!r}\n"
                "payload, status = responses[Path(sys.argv[2]).name]\n"
                "print(json.dumps(payload))\nraise SystemExit(status)\n",
                encoding="utf-8",
            )
            checker.chmod(0o700)
            command = [sys.executable, str(HARNESS), "--bin", str(checker),
                       "--corpus", str(corpus)]
            if report_managed:
                command.append("--report-managed")
            return subprocess.run(command, capture_output=True, text=True, timeout=10)

    def assert_invalid_report(self, managed):
        run = self.run_cases([("ACCEPT", self.accept_payload(managed), 0)])
        self.assertEqual(run.returncode, 1, run.stdout + run.stderr)
        self.assertIn("harness-error", run.stdout)
        self.assertIn("verdict agreement: 0/1", run.stdout)
        self.assertNotIn("Traceback", run.stderr)

    def test_each_counter_requires_nonnegative_integer(self):
        for field in ("managed_blocks", "sites_via_managed", "total_sites"):
            for value in (False, True, -1, 0.5, "0", None):
                with self.subTest(field=field, value=value):
                    managed = {"managed_blocks": 0, "sites_via_managed": 0,
                               "total_sites": 0}
                    managed[field] = value
                    self.assert_invalid_report(managed)

    def test_present_report_requires_all_counters(self):
        for field in ("managed_blocks", "sites_via_managed", "total_sites"):
            with self.subTest(missing=field):
                managed = {"managed_blocks": 0, "sites_via_managed": 0,
                           "total_sites": 0}
                del managed[field]
                self.assert_invalid_report(managed)

    def test_present_report_requires_object_even_when_falsey(self):
        for managed in (False, 0, "", [], [1], "x"):
            with self.subTest(managed=managed):
                self.assert_invalid_report(managed)

    def test_managed_sites_cannot_exceed_total_sites(self):
        self.assert_invalid_report(
            {"managed_blocks": 1, "sites_via_managed": 3, "total_sites": 2}
        )

    def test_valid_zero_report(self):
        run = self.run_cases([("ACCEPT", self.accept_payload(
            {"managed_blocks": 0, "sites_via_managed": 0, "total_sites": 0}
        ), 0)])
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
        self.assertIn("managed aggregate: 0 block(s), 0/0 sites via managed = 0.0%",
                      run.stdout)

    def test_valid_nonzero_report(self):
        run = self.run_cases([("ACCEPT", self.accept_payload(
            {"managed_blocks": 1, "sites_via_managed": 2, "total_sites": 4}
        ), 0)])
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
        self.assertIn("managed aggregate: 1 block(s), 2/4 sites via managed = 50.0%",
                      run.stdout)

    def test_unknown_report_field_keeps_forward_compatibility(self):
        run = self.run_cases([("ACCEPT", self.accept_payload(
            {"managed_blocks": 0, "sites_via_managed": 0, "total_sites": 0,
             "future": {"extra": True}}
        ), 0)])
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
        self.assertIn("verdict agreement: 1/1", run.stdout)

    def test_absent_or_null_report_keeps_parser_reject_contract(self):
        for present in (False, True):
            with self.subTest(report_present=present):
                payload = {
                    "summary": {"verdict": "reject", "errors": 1, "warnings": 0},
                    "diagnostics": [{"severity": "error", "tier0_rule": "A1c",
                                     "code": "HYB-E3002"}],
                }
                if present:
                    payload["summary"]["managed"] = None
                run = self.run_cases([("REJECT A1c", payload, 1)])
                self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
                self.assertIn("verdict agreement: 1/1", run.stdout)
                self.assertNotIn("Traceback", run.stderr)

    def test_report_validation_only_applies_with_report_flag(self):
        for managed in (False, 0, "", [], [1], "x",
                        {"managed_blocks": -1, "sites_via_managed": 5,
                         "total_sites": 2}):
            with self.subTest(managed=managed):
                run = self.run_cases(
                    [("ACCEPT", self.accept_payload(managed), 0)],
                    report_managed=False,
                )
                self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
                self.assertIn("verdict agreement: 1/1", run.stdout)
                self.assertNotIn("managed aggregate:", run.stdout)

    def test_bad_case_does_not_contaminate_later_valid_aggregate(self):
        run = self.run_cases([
            ("ACCEPT", self.accept_payload(
                {"managed_blocks": -1, "sites_via_managed": 5, "total_sites": 2}
            ), 0),
            ("ACCEPT", self.accept_payload(
                {"managed_blocks": 2, "sites_via_managed": 1, "total_sites": 4}
            ), 0),
        ])
        self.assertEqual(run.returncode, 1, run.stdout + run.stderr)
        self.assertIn("harness-error", run.stdout)
        self.assertIn("verdict agreement: 1/2", run.stdout)
        self.assertIn("managed aggregate: 2 block(s), 1/4 sites via managed = 25.0%",
                      run.stdout)
        self.assertNotIn("Traceback", run.stderr)

    def test_valid_reports_aggregate_across_cases(self):
        run = self.run_cases([
            ("ACCEPT", self.accept_payload(
                {"managed_blocks": 1, "sites_via_managed": 2, "total_sites": 4}
            ), 0),
            ("ACCEPT", self.accept_payload(
                {"managed_blocks": 2, "sites_via_managed": 1, "total_sites": 2}
            ), 0),
        ])
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
        self.assertIn("verdict agreement: 2/2", run.stdout)
        self.assertIn("managed aggregate: 3 block(s), 3/6 sites via managed = 50.0%",
                      run.stdout)


class CheckerPathCliTests(unittest.TestCase):
    """The validated --bin file must be the checker that runs (#477).

    `--bin` is verified with os.path.isfile and then handed to
    subprocess.run unchanged. On POSIX an unresolved bare name is
    re-resolved against PATH, so a same-named binary elsewhere on PATH
    can silently replace the validated local checker — a false verdict
    agreement or a false disagreement — and a PATH without such a binary
    raises FileNotFoundError even though the local file passed
    validation. POSIX shebang checkers stand in for hyb; the scenarios
    mirror the issue's local/PATH directory split.
    """

    REJECT_PAYLOAD = (
        "{'summary': {'verdict': 'reject'}, "
        "'diagnostics': [{'severity': 'error', 'tier0_rule': 'A1c'}]}"
    )
    ACCEPT_PAYLOAD = "{'summary': {'verdict': 'accept'}, 'diagnostics': []}"

    def make_checker(self, path, payload, status):
        path.write_text(
            f"#!{sys.executable}\nimport json\n"
            f"print(json.dumps({payload}))\nraise SystemExit({status})\n",
            encoding="utf-8",
        )
        path.chmod(0o700)

    def run_shadow(self, expected, local_payload, local_status, path_payload,
                   path_status, bin_name="hyb", path_value=None):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            corpus = root / "corpus"
            corpus.mkdir()
            (corpus / "01-example.hy").write_text(
                # REJECT carries its rule; a rule after ACCEPT is a corpus
                # error by design since #349's ambiguous-header validation.
                f"// EXPECTED-VERDICT: {expected}" + (" A1c" if expected == "REJECT" else "") + f"\nfn main() {{}}\n",
                encoding="utf-8",
            )
            local = root / "local"
            local.mkdir()
            self.make_checker(local / "hyb", local_payload, local_status)
            shadow = root / "shadow"
            shadow.mkdir()
            self.make_checker(shadow / "hyb", path_payload, path_status)
            env = dict(os.environ)
            env["PATH"] = str(shadow) if path_value is None else path_value
            return subprocess.run(
                [sys.executable, str(HARNESS), "--bin", bin_name,
                 "--corpus", str(corpus)],
                capture_output=True, text=True, timeout=10, env=env,
                cwd=str(local),
            )

    def test_path_checker_cannot_replace_the_validated_local_one(self):
        # Local rejects, PATH shadow accepts, fixture expects REJECT: the
        # shadow must not turn the verdict into a disagreement.
        run = self.run_shadow("REJECT", self.REJECT_PAYLOAD, 1,
                              self.ACCEPT_PAYLOAD, 0)
        self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
        self.assertIn("verdict agreement: 1/1", run.stdout)

    def test_path_checker_cannot_fake_agreement(self):
        # The dangerous direction: local rejects, the PATH shadow accepts,
        # and an ACCEPT fixture would report a false 1/1 agreement.
        run = self.run_shadow("ACCEPT", self.REJECT_PAYLOAD, 1,
                              self.ACCEPT_PAYLOAD, 0)
        self.assertEqual(run.returncode, 1, run.stdout + run.stderr)
        self.assertIn("verdict agreement: 0/1", run.stdout)

    def test_absolute_bin_runs_the_validated_file(self):
        # An absolute --bin is unchanged by the resolution (idempotence):
        # the PATH shadow still must not win.
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            corpus = root / "corpus"
            corpus.mkdir()
            (corpus / "01-example.hy").write_text(
                "// EXPECTED-VERDICT: REJECT A1c\nfn main() {}\n",
                encoding="utf-8",
            )
            local = root / "local"
            local.mkdir()
            self.make_checker(local / "hyb", self.REJECT_PAYLOAD, 1)
            shadow = root / "shadow"
            shadow.mkdir()
            self.make_checker(shadow / "hyb", self.ACCEPT_PAYLOAD, 0)
            env = dict(os.environ)
            env["PATH"] = str(shadow)
            run = subprocess.run(
                [sys.executable, str(HARNESS), "--bin", str(local / "hyb"),
                 "--corpus", str(corpus)],
                capture_output=True, text=True, timeout=10, env=env,
            )
            self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
            self.assertIn("verdict agreement: 1/1", run.stdout)

    def test_path_without_the_checker_runs_the_local_file(self):
        # A PATH that carries no such binary used to raise
        # FileNotFoundError after the local file passed validation. The
        # empty PATH dir is created inside this test's own root so the
        # scenario cannot silently stop testing the clean-PATH branch.
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            corpus = root / "corpus"
            corpus.mkdir()
            (corpus / "01-example.hy").write_text(
                "// EXPECTED-VERDICT: REJECT A1c\nfn main() {}\n",
                encoding="utf-8",
            )
            local = root / "local"
            local.mkdir()
            self.make_checker(local / "hyb", self.REJECT_PAYLOAD, 1)
            empty_path = root / "empty-path"
            empty_path.mkdir()
            env = dict(os.environ)
            env["PATH"] = str(empty_path)
            run = subprocess.run(
                [sys.executable, str(HARNESS), "--bin", "hyb",
                 "--corpus", str(corpus)],
                capture_output=True, text=True, timeout=10, env=env,
                cwd=str(local),
            )
            self.assertEqual(run.returncode, 0, run.stdout + run.stderr)
            self.assertIn("verdict agreement: 1/1", run.stdout)
            self.assertNotIn("Traceback", run.stderr)

    def test_non_executable_bin_reports_harness_error(self):
        # isfile proves existence, not executability: a validated but
        # non-executable --bin must surface as a harness-error row, not
        # an uncaught PermissionError traceback.
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            corpus = root / "corpus"
            corpus.mkdir()
            (corpus / "01-example.hy").write_text(
                "// EXPECTED-VERDICT: REJECT A1c\nfn main() {}\n",
                encoding="utf-8",
            )
            local = root / "local"
            local.mkdir()
            checker = local / "hyb"
            self.make_checker(checker, self.REJECT_PAYLOAD, 1)
            checker.chmod(0o644)
            run = subprocess.run(
                [sys.executable, str(HARNESS), "--bin", str(checker),
                 "--corpus", str(corpus)],
                capture_output=True, text=True, timeout=10,
            )
            self.assertEqual(run.returncode, 1, run.stdout + run.stderr)
            self.assertIn("harness-error", run.stdout)
            self.assertIn("verdict agreement: 0/1", run.stdout)
            self.assertNotIn("Traceback", run.stderr)

    def test_missing_bin_still_fails_closed(self):
        with tempfile.TemporaryDirectory() as tmp:
            run = subprocess.run(
                [sys.executable, str(HARNESS), "--bin",
                 str(Path(tmp) / "absent" / "hyb"), "--corpus", str(Path(tmp))],
                capture_output=True, text=True, timeout=10)
            self.assertEqual(run.returncode, 2, run.stdout + run.stderr)
            self.assertIn("checker binary not found", run.stderr)


class CheckerProcessFailureCliTests(unittest.TestCase):
    """A checker process that hangs or cannot launch is a per-case failure
    row; the remaining corpus fixtures still run (pr-198, shoujianing).
    Deterministic: corpus_harness.subprocess.run is patched, first call fails,
    second call returns an accepting payload."""

    def check_failure(self, failure, message):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            checker = root / "hyb"
            checker.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            checker.chmod(0o700)
            for name in ("01-first.hy", "02-next.hy"):
                (root / name).write_text(
                    "// EXPECTED-VERDICT: ACCEPT\nfn main() {}\n", encoding="utf-8"
                )
            accepted = subprocess.CompletedProcess(
                ["hyb"], 0,
                json.dumps({"summary": {"verdict": "accept"},
                            "diagnostics": []}).encode("utf-8"),
                b"",
            )
            output = io.StringIO()
            with patch.object(
                     sys, "argv",
                     ["corpus_harness.py", "--bin", str(checker),
                      "--corpus", str(root)]), \
                 patch.object(corpus_harness.subprocess, "run",
                              side_effect=[failure, accepted]) as run, \
                 contextlib.redirect_stdout(output):
                status = corpus_harness.main()
            self.assertEqual(status, 1)
            self.assertEqual(run.call_count, 2, output.getvalue())
            self.assertIn(message, output.getvalue())
            self.assertIn("verdict agreement: 1/2", output.getvalue())
            self.assertIn("harness-error", output.getvalue())

    def test_timeout_is_a_reported_failure_and_next_case_runs(self):
        self.check_failure(
            subprocess.TimeoutExpired(["hyb"], 60), "timed out")

    def test_launch_error_is_a_reported_failure_and_next_case_runs(self):
        self.check_failure(
            OSError(2, "No such file or directory"), "could not be executed")


class CorpusFailClosedIssue118Tests(unittest.TestCase):
    """Fail-closed guards for unclassified Tier-0 errors and unreadable corpus paths (#118)."""

    FIXTURE_NAMES = (
        "issue_118_unclassified_diag_1.json",
        "issue_118_unclassified_diag_2.json",
        "issue_118_whitespace_rule_3.json",
        "issue_118_nonstring_rule_4.json",
    )

    def test_unclassified_and_invalid_tier0_rule_fixtures_fail_closed(self):
        fixtures_dir = Path(__file__).with_name("tests")
        for fname in self.FIXTURE_NAMES:
            with self.subTest(fixture=fname):
                case = json.loads((fixtures_dir / fname).read_text(encoding="utf-8"))
                got = corpus_harness.classify_got(case["summary"], case["diagnostics"])
                self.assertEqual(got, ("reject", case["classified_rule"], []))
                ok, why = corpus_harness.agree(tuple(case["expected"]), got)
                self.assertIs(ok, case["agree"])
                self.assertIn("expected reject", why)
        for bad_rule in (None, "", "   ", 123, "<unclassified-error>"):
            with self.subTest(bad_rule=bad_rule):
                ok, _ = corpus_harness.agree(
                    ("REJECT", "<unclassified-error>"), ("reject", bad_rule, [])
                )
                self.assertFalse(ok)
        got_valid = corpus_harness.classify_got(
            {"verdict": "reject"}, [{"severity": "error", "tier0_rule": " A1c "}]
        )
        self.assertEqual(got_valid, ("reject", "A1c", []))
        self.assertEqual(
            corpus_harness.agree(("REJECT", "A1c"), got_valid),
            (True, "expected reject A1c, got reject A1c"),
        )

    def test_unreadable_corpus_file_raises_value_error_and_exits_2(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            checker = root / "hyb"
            checker.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            bad_utf8 = root / "01-bad.hy"
            bad_utf8.write_bytes(b"\xff\xfe\x00")
            dir_entry = root / "02-dir.hy"
            dir_entry.mkdir()
            for target in (bad_utf8, dir_entry):
                with self.subTest(target=target.name):
                    with self.assertRaisesRegex(ValueError, "cannot read corpus file"):
                        corpus_harness.read_expected(str(target))
                    with self.assertRaisesRegex(ValueError, "cannot read corpus file"):
                        corpus_harness.read_expected_diagnostic(
                            str(target), ("REJECT", "A1c")
                        )
            err = io.StringIO()
            with patch.object(
                sys,
                "argv",
                ["corpus_harness.py", "--bin", str(checker), "--corpus", str(root)],
            ), contextlib.redirect_stderr(err):
                status = corpus_harness.main()
            self.assertEqual(status, 2)
            self.assertIn("cannot read corpus file", err.getvalue())

    def test_unreadable_or_missing_corpus_directory_exits_2(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            checker = root / "hyb"
            checker.write_text("#!/bin/sh\nexit 0\n", encoding="utf-8")
            missing_corpus = root / "missing-corpus"
            err = io.StringIO()
            with patch.object(
                sys,
                "argv",
                [
                    "corpus_harness.py",
                    "--bin",
                    str(checker),
                    "--corpus",
                    str(missing_corpus),
                ],
            ), contextlib.redirect_stderr(err):
                status = corpus_harness.main()
            self.assertEqual(status, 2)
            self.assertIn("error: corpus directory not readable:", err.getvalue())


if __name__ == "__main__":
    unittest.main()
