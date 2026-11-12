"""Offline regression tests: posting a board must not overwrite a marker quote."""
import contextlib
import copy
import importlib.util
import io
from pathlib import Path
from types import SimpleNamespace
import unittest
from unittest.mock import patch


SPEC = importlib.util.spec_from_file_location(
    "triage_board_subject", Path(__file__).with_name("triage_bot.py")
)
TRIAGE = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(TRIAGE)
TITLE = "Triage Board (auto-updated daily — pin me)"
MARKER = "<!-- triage-bot:board:v1 -->"


def issue(number=42, **overrides):
    result = {"number": number, "title": TITLE, "body": "# Triage Board\n\n" + MARKER,
              "state": "open", "user": {"login": "maintainer"},
              "author_association": "OWNER"}
    result.update(overrides)
    return result


class FakeGitHub:
    repo = "fixture/repo"
    calls = 0
    remaining = 1000

    def __init__(self, hits=(), details=None, **search_overrides):
        self.search = {"items": list(hits), "total_count": len(hits),
                       "incomplete_results": False, **search_overrides}
        self.details = details or {item["number"]: item for item in hits}
        self.reads, self.writes = [], []

    def req(self, method, path, body=None):
        if method != "GET":
            raise AssertionError("unexpected request: " + method)
        self.reads.append(path)
        if path.startswith("/search/issues?"):
            return 200, copy.deepcopy(self.search)
        prefix = "/repos/fixture/repo/issues/"
        if path.startswith(prefix):
            value = self.details.get(int(path[len(prefix):]))
            return (200, copy.deepcopy(value)) if value else (404, None)
        raise AssertionError("unexpected path: " + path)

    def patch(self, path, body):
        self.writes.append(("PATCH", path, body))
        return {}

    def post(self, path, body):
        self.writes.append(("POST", path, body))
        return {"number": 43, "html_url": "https://example.invalid/issues/43"}


class BoardTarget(unittest.TestCase):
    def run_board(self, gh, *, board_issue=None, post=True):
        cfg = TRIAGE.load_cfg()
        cfg["repo"] = gh.repo
        cfg.pop("board_issue", None)
        if board_issue is not None:
            cfg["board_issue"] = board_issue
        args = SimpleNamespace(pareto=None, from_scan=None, post=post, jobs=1)
        with patch.object(TRIAGE, "load_cfg", return_value=cfg), \
                patch.object(TRIAGE, "load_token", return_value="offline-unused"), \
                patch.object(TRIAGE, "Gh", return_value=gh), \
                patch.object(TRIAGE, "collect", return_value=([], "a" * 40)), \
                contextlib.redirect_stdout(io.StringIO()):
            TRIAGE.cmd_board(args)

    def assert_no_write(self, gh, **kwargs):
        with self.assertRaises(RuntimeError):
            self.run_board(gh, **kwargs)
        self.assertEqual(gh.writes, [])

    def test_configured_target_is_used_without_search(self):
        gh = FakeGitHub([issue(99)], details={42: issue()})
        self.run_board(gh, board_issue=42)
        self.assertEqual(gh.reads, ["/repos/fixture/repo/issues/42"])
        self.assertEqual(gh.writes[0][:2], ("PATCH", "/repos/fixture/repo/issues/42"))

    def test_wrong_search_hit_first_does_not_receive_the_board(self):
        unrelated = issue(99, title="A bug report quoting the board marker",
                          user={"login": "contributor"}, author_association="NONE")
        gh = FakeGitHub([unrelated, issue()])
        self.run_board(gh)
        self.assertEqual(gh.writes[0][:2], ("PATCH", "/repos/fixture/repo/issues/42"))

    def test_copied_title_and_marker_do_not_establish_board_ownership(self):
        copied = issue(99, user={"login": "contributor"}, author_association="NONE")
        gh = FakeGitHub([copied, issue()])
        self.run_board(gh)
        self.assertEqual(gh.writes[0][:2], ("PATCH", "/repos/fixture/repo/issues/42"))

    def test_multiple_trusted_candidates_fail_closed(self):
        self.assert_no_write(FakeGitHub([issue(42), issue(43)]))

    def test_missing_configured_target_does_not_create_or_fall_back(self):
        self.assert_no_write(FakeGitHub([issue(99)]), board_issue=42)

    def test_invalid_configured_target_fails_closed(self):
        for changed in ({"state": "closed"}, {"body": "Ordinary issue"},
                        {"pull_request": {"url": "https://example.invalid/pr/42"}}):
            with self.subTest(changed=changed):
                gh = FakeGitHub([issue(99)], details={42: issue(**changed)})
                self.assert_no_write(gh, board_issue=42)

    def test_invalid_configured_number_fails_before_requests(self):
        for number in (0, -1, True, "42"):
            with self.subTest(number=number):
                gh = FakeGitHub()
                self.assert_no_write(gh, board_issue=number)
                self.assertEqual(gh.reads, [])

    def test_discovered_target_is_rechecked_before_writing(self):
        gh = FakeGitHub([issue()], details={42: issue(state="closed")})
        self.assert_no_write(gh)

    def test_incomplete_or_truncated_search_fails_closed(self):
        for change in ({"incomplete_results": True}, {"total_count": 2}):
            with self.subTest(change=change):
                self.assert_no_write(FakeGitHub([issue()], **change))

    def test_unconfigured_empty_search_creates_a_board(self):
        gh = FakeGitHub()
        self.run_board(gh)
        self.assertEqual(gh.writes[0][:2], ("POST", "/repos/fixture/repo/issues"))
        self.assertEqual(gh.writes[0][2]["title"], TITLE)
        self.assertIn(MARKER, gh.writes[0][2]["body"])

    def test_actions_authored_board_can_be_discovered(self):
        board = issue(user={"login": "github-actions[bot]"}, author_association="NONE")
        gh = FakeGitHub([board])
        self.run_board(gh)
        self.assertEqual(gh.writes[0][:2], ("PATCH", "/repos/fixture/repo/issues/42"))

    def test_print_only_never_selects_or_writes_an_issue(self):
        gh = FakeGitHub([issue(99)])
        self.run_board(gh, board_issue=42, post=False)
        self.assertEqual(gh.reads, [])
        self.assertEqual(gh.writes, [])


class ChunkedForegroundScanTests(unittest.TestCase):
    def make_pr(self, number, sha="abcdef1234567890"):
        return {
            "number": number,
            "title": "PR #%d" % number,
            "user": {"login": "dev"},
            "created_at": "2026-11-09T00:00:00Z",
            "updated_at": "2026-11-09T01:00:00Z",
            "draft": False,
            "labels": [],
            "head": {"sha": sha},
        }

    def test_limit_and_since_pr_bound_scan_and_resume_reuses_cached_head(self):
        import json
        import tempfile

        prs = [self.make_pr(4230, "1111111111110000"), self.make_pr(4220, "2222222222220000"), self.make_pr(4100, "3333333333330000")]
        detail_calls = []

        class ScanFakeGh:
            repo = "fixture/repo"
            calls = 0
            remaining = 999

            def paginate(self, path, max_pages=20, key=None, stop_when=None):
                if "/pulls?state=open" in path:
                    if stop_when:
                        stop_when(prs, prs)
                    return list(prs)
                if "/files" in path:
                    return [{"filename": "compiler/crates/hy-check/tests/t.rs", "additions": 10, "deletions": 0}]
                return []

            def get(self, path):
                if path.endswith("/commits/main"):
                    return {"sha": "mainsha1234567890"}
                if "/pulls/" in path:
                    detail_calls.append(path)
                    return {"mergeable": True, "mergeable_state": "clean"}
                if "/compare/" in path:
                    return {"behind_by": 0, "merge_base_commit": {"sha": "mainsha1234567890"}}
                return {}

        with tempfile.TemporaryDirectory() as tmp:
            gh = ScanFakeGh()
            args1 = SimpleNamespace(limit=2, since_pr=4200, resume=False, out=tmp, jobs=1)
            with contextlib.redirect_stdout(io.StringIO()):
                items1, main_sha = TRIAGE.collect({}, gh, args1)
            self.assertEqual([it["number"] for it in items1], [4230, 4220])
            self.assertEqual(len(detail_calls), 2)

            # Write TRIAGE.json and verify --resume skips per-PR detail calls when head_sha matches
            (Path(tmp) / "TRIAGE.json").write_text(
                json.dumps({"main": main_sha, "items": items1})
            )
            detail_calls.clear()
            args2 = SimpleNamespace(limit=2, since_pr=4200, resume=True, out=tmp, jobs=1)
            with contextlib.redirect_stdout(io.StringIO()):
                items2, _ = TRIAGE.collect({}, gh, args2)
            self.assertEqual([it["number"] for it in items2], [4230, 4220])
            self.assertEqual(detail_calls, [])


if __name__ == "__main__":
    unittest.main()
