"""Offline regressions for #4825: REST mergeable=null in queue_snapshot and
green-CI / Shepherd-label guards in pr_triage_runner."""
import contextlib
from datetime import datetime, timedelta, timezone
import importlib.util
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

TOOLS_DIR = Path(__file__).resolve().parent
NOW = datetime(2026, 11, 27, 6, 0, tzinfo=timezone.utc)


def stamp(dt):
    return dt.isoformat().replace("+00:00", "Z")


class FakeResponse:
    def __init__(self, payload, status=200):
        self._raw = json.dumps(payload).encode("utf-8")
        self.status = status

    def read(self):
        return self._raw

    def __enter__(self):
        return self

    def __exit__(self, exc_type, exc, tb):
        return False


def load_runner():
    spec = importlib.util.spec_from_file_location(
        "pr_triage_runner", TOOLS_DIR / "pr_triage_runner.py"
    )
    mod = importlib.util.module_from_spec(spec)
    with patch.dict(os.environ, {"GH_TOKEN": "offline-fixture"}, clear=False):
        spec.loader.exec_module(mod)
    return mod


def run_snapshot_with_fixtures(gql_nodes, rest_by_number):
    with tempfile.TemporaryDirectory() as tmp:
        out_path = Path(tmp) / "queue_snapshot.json"

        def fake_urlopen(req, timeout=60):
            url = req.full_url
            if url.endswith("/graphql"):
                return FakeResponse({
                    "data": {
                        "repository": {
                            "pullRequests": {
                                "pageInfo": {"hasNextPage": False, "endCursor": None},
                                "nodes": gql_nodes,
                            }
                        }
                    }
                })
            for num, payload in rest_by_number.items():
                if url.endswith(f"/pulls/{num}"):
                    return FakeResponse(payload)
            raise AssertionError(f"unexpected URL: {url}")

        spec = importlib.util.spec_from_file_location(
            "queue_snapshot", TOOLS_DIR / "queue_snapshot.py"
        )
        mod = importlib.util.module_from_spec(spec)
        buf = io.StringIO()
        env = {"GH_TOKEN": "offline-fixture", "SURVEY_OUT": str(out_path)}
        with patch.dict(os.environ, env, clear=False), \
             patch("urllib.request.urlopen", side_effect=fake_urlopen), \
             patch("time.sleep"), \
             contextlib.redirect_stdout(buf):
            spec.loader.exec_module(mod)
            if hasattr(mod, "main") and not out_path.is_file():
                mod.main()
        return json.loads(out_path.read_text(encoding="utf-8"))


class TriageNullMergeableTests(unittest.TestCase):
    def gql_pr(self, number, *, ci="SUCCESS", mergeable="UNKNOWN",
               merge_state="UNKNOWN", labels=(), age_days=14, idle_days=12):
        return {
            "number": number,
            "title": f"fix(hy-check): sample fix #{number}",
            "createdAt": stamp(NOW - timedelta(days=age_days)),
            "updatedAt": stamp(NOW - timedelta(days=idle_days)),
            "isDraft": False,
            "author": {"login": "contributor"},
            "mergeable": mergeable,
            "mergeStateStatus": merge_state,
            "labels": {"nodes": [{"name": name} for name in labels]},
            "comments": {"totalCount": 0},
            "commits": {
                "nodes": [
                    {
                        "commit": {
                            "statusCheckRollup": {"state": ci} if ci != "NONE" else None
                        }
                    }
                ]
            },
        }

    def survey_pr(self, number=101, *, ci="SUCCESS", mergeable="CONFLICTING",
                  merge_state="DIRTY", labels=(), age_days=14.0, idle_days=12.0):
        return {
            "number": number,
            "title": f"fix(hy-check): sample fix #{number}",
            "createdAt": stamp(NOW - timedelta(days=age_days)),
            "updatedAt": stamp(NOW - timedelta(days=idle_days)),
            "isDraft": False,
            "author": {"login": "contributor"},
            "mergeable": mergeable,
            "mergeStateStatus": merge_state,
            "_age_days": age_days,
            "_idle_days": idle_days,
            "_ci_rollup": ci,
            "_suite_concls": {},
            "_labels": list(labels),
        }

    def test_snapshot_preserves_unknown_when_rest_mergeable_is_null(self):
        nodes = [
            self.gql_pr(101),
            self.gql_pr(102),
            self.gql_pr(103),
        ]
        rest_payloads = {
            101: {"mergeable": None, "mergeable_state": "unknown"},
            102: {"mergeable": True, "mergeable_state": "clean"},
            103: {"mergeable": False, "mergeable_state": "dirty"},
        }
        snapped = {p["number"]: p for p in run_snapshot_with_fixtures(nodes, rest_payloads)}
        self.assertEqual(snapped[101]["mergeable"], "UNKNOWN")
        self.assertEqual(snapped[101]["mergeStateStatus"], "UNKNOWN")
        self.assertEqual(snapped[102]["mergeable"], "MERGEABLE")
        self.assertEqual(snapped[102]["mergeStateStatus"], "CLEAN")
        self.assertEqual(snapped[103]["mergeable"], "CONFLICTING")
        self.assertEqual(snapped[103]["mergeStateStatus"], "DIRTY")

    def test_decide_never_closes_green_or_pending_ci_prs(self):
        runner = load_runner()
        with patch.object(runner, "config_protected", return_value=set()):
            for ci_state in ("SUCCESS", "PENDING"):
                pr = self.survey_pr(201, ci=ci_state, mergeable="CONFLICTING", idle_days=15.0)
                action, evidence, extra = runner.decide(pr)
                self.assertEqual((action, evidence, extra), ("skip", [], ""))

    def test_decide_protects_shepherd_approved_labels(self):
        runner = load_runner()
        with patch.object(runner, "config_protected", return_value=set()):
            for label in ("bot:ready-to-land", "bot:verified", "bot:human-needed"):
                pr = self.survey_pr(202, ci="FAILURE", mergeable="CONFLICTING",
                                    labels=(label,), idle_days=15.0)
                action, evidence, _ = runner.decide(pr)
                self.assertEqual((action, evidence), ("skip", []))

    def test_decide_still_closes_red_or_unverified_conflicting_prs(self):
        runner = load_runner()
        with patch.object(runner, "config_protected", return_value=set()):
            red = self.survey_pr(203, ci="FAILURE", mergeable="MERGEABLE", idle_days=6.0)
            self.assertEqual(runner.decide(red)[:2], ("close", ["ci-failure"]))

            stale_conf = self.survey_pr(204, ci="NONE", mergeable="CONFLICTING", idle_days=12.0)
            self.assertEqual(runner.decide(stale_conf)[:2], ("close", ["conflicts"]))

            young = self.survey_pr(205, ci="FAILURE", age_days=2.0, idle_days=2.0)
            self.assertEqual(runner.decide(young), ("skip", [], ""))


if __name__ == "__main__":
    unittest.main()
