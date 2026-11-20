"""Offline regressions for #3051: config, input boundaries and write failures.

Use a disposable installation, fixed clock/environment and an API fixture.
Network entry points are denied even if the target catches their exceptions.
"""
import contextlib
from datetime import datetime, timedelta, timezone
import importlib.util
import io
import json
import os
from pathlib import Path
import shutil
import socket
import sys
import tempfile
import unittest
from unittest.mock import patch
import urllib.request


NOW = datetime(2026, 10, 24, 15, tzinfo=timezone.utc)
SOURCE = Path(__file__).with_name("queue_sweeper.py")


class FixedDateTime(datetime):
    @classmethod
    def now(cls, tz=None):
        return NOW if tz is not None else NOW.replace(tzinfo=None)


def pr(number, age=20, labels=()):
    stamp = (NOW - timedelta(days=age)).isoformat()
    return {"number": number, "created_at": stamp, "updated_at": stamp,
            "labels": [{"name": name} for name in labels],
            "title": "Offline contribution", "user": {"login": "fixture"}}


class SweeperSafetyTests(unittest.TestCase):
    def setUp(self):
        private_temp = tempfile.gettempdir()
        self.stack = contextlib.ExitStack()
        self.addCleanup(self.stack.close)
        self.stack.enter_context(patch.dict(os.environ, {
            "GITHUB_TOKEN": "offline-fixture", "GITHUB_REPOSITORY": "example/project",
            "SWEEPER_MIN_AGE_DAYS": "5"}, clear=True))
        self.network_attempts = []

        def denied(*args, **kwargs):
            self.network_attempts.append("network attempted")
            raise AssertionError("real network forbidden")

        for owner, name in ((urllib.request, "urlopen"), (socket, "create_connection"),
                            (socket, "socket")):
            self.stack.enter_context(patch.object(owner, name, side_effect=denied))
        self.root = Path(self.stack.enter_context(tempfile.TemporaryDirectory(
            prefix="sweeper-safety-", dir=private_temp)))
        self.install = self.root / "installation" / "tools"
        self.install.mkdir(parents=True)
        shutil.copyfile(SOURCE, self.install / "queue_sweeper.py")
        self.config({})
        old_cwd = Path.cwd()
        self.stack.callback(os.chdir, old_cwd)
        os.chdir(self.root)
        self.out = io.StringIO()
        self.stack.enter_context(contextlib.redirect_stdout(self.out))
        self.stack.enter_context(contextlib.redirect_stderr(self.out))
        self.calls = []

    def tearDown(self):
        self.assertEqual(self.network_attempts, [])

    def config(self, value):
        (self.install / "triage_config.json").write_text(json.dumps(value), encoding="utf-8")

    def load(self, env="5"):
        if env is None:
            os.environ.pop("SWEEPER_MIN_AGE_DAYS", None)
        else:
            os.environ["SWEEPER_MIN_AGE_DAYS"] = env
        spec = importlib.util.spec_from_file_location("safety_sweeper", self.install / "queue_sweeper.py")
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        self.stack.enter_context(patch.object(module, "datetime", FixedDateTime))
        self.stack.enter_context(patch.object(module.time, "sleep"))
        self.stack.enter_context(patch.object(module.time, "time", return_value=123456.0))
        return module

    def sweep(self, args=(), prs=None, close=None, comment=None, env="5"):
        module = self.load(env)
        close = close or {}
        comment = comment or {}

        def offline_api(path, method="GET", body=None):
            self.calls.append((path, method, body))
            if method == "PATCH":
                number = int(path.rsplit("/", 1)[1])
                self.assertEqual(path, f"/repos/example/project/pulls/{number}")
                self.assertEqual(body["state"], "closed")
                return close.get(number, 200), {}
            if method == "POST":
                number = int(path.split("/issues/")[1].split("/")[0])
                self.assertEqual(path, f"/repos/example/project/issues/{number}/comments")
                self.assertTrue(body["body"].strip())
                return comment.get(number, 201), {}
            self.assertEqual(method, "GET")
            if path == "/repos/example/project/pulls?state=open&per_page=100&page=1":
                return 200, [pr(81)] if prs is None else prs
            if "/events?" in path:
                return 200, []
            if path == "/repos/example/project":
                return 200, {"open_issues_count": 0}
            self.fail(f"unexpected API call: {method} {path}")

        with patch.object(module, "api", side_effect=offline_api), \
             patch.object(sys, "argv", ["queue_sweeper", *args]):
            module.main()

    def writes(self):
        return [(path, method, body) for path, method, body in self.calls if method != "GET"]

    def summary(self, closed, fails):
        self.assertIn(f"DONE: closed={closed} fails={fails} ", self.out.getvalue())

    def test_config_uses_installation_from_tools_and_decoy_cwd(self):
        self.config({"wildcard_holds": [81], "drafts_parked": [82]})
        (self.root / "tools").mkdir()
        (self.root / "tools/triage_config.json").write_text('{"wildcard_holds":[83]}', encoding="utf-8")
        for cwd in (self.root, self.install):
            with self.subTest(cwd=cwd):
                os.chdir(cwd)
                self.assertEqual(self.load().config_protected_numbers(), {81, 82})
                self.calls.clear()
                self.sweep(prs=[pr(81), pr(82), pr(83)])
                self.assertEqual([p for p, m, _ in self.writes() if m == "PATCH"],
                                 ["/repos/example/project/pulls/83"])

    def test_config_filters_each_entry_without_losing_valid_siblings(self):
        self.config({"wildcard_holds": [81, [], {}, "82", True, False, None, 83.0, 81],
                     "drafts_parked": [84, {}, 0, -1]})
        result = self.load().config_protected_numbers()
        self.assertEqual(result, {81, 84, 0, -1})
        self.assertTrue(all(type(n) is int for n in result))

    def test_wrong_shaped_field_does_not_discard_other_list(self):
        for field, other in (("wildcard_holds", "drafts_parked"),
                             ("drafts_parked", "wildcard_holds")):
            for invalid in ("81", 81, True, None, {"81": True}):
                with self.subTest(field=field, invalid=invalid):
                    self.config({field: invalid, other: [82]})
                    self.assertEqual(self.load().config_protected_numbers(), {82})

    def test_missing_keys_contribute_nothing(self):
        self.assertEqual(self.load().config_protected_numbers(), set())

    def test_missing_and_malformed_config_fall_back(self):
        path = self.install / "triage_config.json"
        path.unlink()
        self.assertEqual(self.load().config_protected_numbers(), set())
        path.write_text("{broken", encoding="utf-8")
        self.assertEqual(self.load().config_protected_numbers(), set())

    def test_unreadable_config_falls_back(self):
        module = self.load()
        with patch("builtins.open", side_effect=PermissionError("fixture denial")):
            self.assertEqual(module.config_protected_numbers(), set())

    def test_labels_still_protect_when_config_is_missing(self):
        (self.install / "triage_config.json").unlink()
        self.sweep(prs=[pr(81, labels=["triage:wildcard-hold"]),
                        pr(82, labels=["triage:pareto"]), pr(83, labels=["triage:keep"])])
        self.summary(0, 0)
        self.assertEqual(self.writes(), [])

    def test_invalid_cli_rejects_before_api_including_dry(self):
        for option, values in (("--min-age", ("0", "-1", "1.5", "bad")),
                               ("--max", ("-1", "1.5", "bad"))):
            for value in values:
                for dry in ((), ("--dry",)):
                    with self.subTest(option=option, value=value, dry=dry):
                        self.calls.clear()
                        with self.assertRaises(SystemExit) as caught:
                            self.sweep(args=(*dry, option, value))
                        self.assertNotIn(caught.exception.code, (None, 0))
                        self.assertEqual(self.calls, [])

    def test_invalid_env_rejects_before_api_including_dry(self):
        for value in ("0", "-3", "bad", "1.5"):
            for args in ((), ("--dry",)):
                with self.subTest(value=value, args=args):
                    self.calls.clear()
                    with self.assertRaises((SystemExit, ValueError)) as caught:
                        self.sweep(args=args, env=value)
                    if isinstance(caught.exception, SystemExit):
                        self.assertNotIn(caught.exception.code, (None, 0))
                    self.assertEqual(self.calls, [])

    def test_default_age_and_exact_cutoff(self):
        self.assertEqual(self.load(env=None).MIN_AGE, 5)
        older = pr(81, age=5)
        older["created_at"] = (NOW - timedelta(days=5, seconds=1)).isoformat()
        older["updated_at"] = older["created_at"]
        self.sweep(prs=[older, pr(82, age=5), pr(83, age=0)])
        self.summary(1, 0)
        self.assertEqual([p for p, m, _ in self.writes() if m == "PATCH"],
                         ["/repos/example/project/pulls/81"])

    def test_explicit_age_overrides_valid_environment(self):
        self.sweep(args=("--min-age", "3"), env="7", prs=[pr(81, age=4)])
        self.summary(1, 0)
        reply = self.writes()[1][2]["body"]
        self.assertIn("review window (3 days)", reply)
        self.assertIn("reopen", reply)

    def test_zero_cap_performs_no_writes(self):
        self.sweep(args=("--max", "0"))
        self.summary(0, 0)
        self.assertEqual(self.writes(), [])

    def test_positive_cap_limits_attempts(self):
        self.sweep(args=("--max", "1"), prs=[pr(81), pr(82)])
        self.summary(1, 0)
        self.assertEqual(len(self.writes()), 2)

    def test_dry_lists_due_without_writes(self):
        self.sweep(args=("--dry",))
        self.assertIn("due for close: 1", self.out.getvalue())
        self.assertIn("DRY RUN", self.out.getvalue())
        self.assertEqual(self.writes(), [])

    def test_only_close_200_allows_comment_and_later_pr_continues(self):
        for status in (201, 204, 403, 422, 500, 0):
            with self.subTest(status=status):
                self.calls.clear()
                self.out.seek(0)
                self.out.truncate()
                self.sweep(prs=[pr(81), pr(82)], close={81: status})
                self.summary(1, 1)
                self.assertEqual([(p, m) for p, m, _ in self.writes()], [
                    ("/repos/example/project/pulls/81", "PATCH"),
                    ("/repos/example/project/pulls/82", "PATCH"),
                    ("/repos/example/project/issues/82/comments", "POST")])

    def test_only_comment_201_completes_operation(self):
        for status in (200, 204, 403, 422, 500, 0):
            with self.subTest(status=status):
                self.calls.clear()
                self.out.seek(0)
                self.out.truncate()
                self.sweep(prs=[pr(81), pr(82)], comment={81: status})
                self.summary(1, 1)
                self.assertEqual([m for _, m, _ in self.writes()], ["PATCH", "POST", "PATCH", "POST"])

    def test_mixed_batch_counts_each_attempt_once(self):
        self.sweep(prs=[pr(n) for n in range(81, 87)],
                   close={81: 422, 84: 0}, comment={82: 403, 85: 422})
        self.summary(2, 4)
        self.assertEqual([p for p, m, _ in self.writes() if m == "POST"],
                         [f"/repos/example/project/issues/{n}/comments" for n in (82, 83, 85, 86)])


if __name__ == "__main__":
    unittest.main()
