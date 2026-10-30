"""Offline regression tests against this candidate's real collect/Gh/classify/render."""

import copy
import importlib.util
import io
import json
import tempfile
import unittest
from datetime import datetime, timedelta, timezone
from pathlib import Path
from threading import Barrier
from types import SimpleNamespace
from unittest.mock import patch
from urllib.error import HTTPError
from urllib.parse import urlsplit

ROOT = Path(__file__).resolve().parent

spec = importlib.util.spec_from_file_location("candidate_triage_bot", ROOT / "triage_bot.py")
bot = importlib.util.module_from_spec(spec)
spec.loader.exec_module(bot)
shepherd_spec = importlib.util.spec_from_file_location("candidate_shepherd", ROOT / "shepherd.py")
shepherd = importlib.util.module_from_spec(shepherd_spec)
shepherd_spec.loader.exec_module(shepherd)

CFG = dict(wildcard_holds=[], drafts_parked=[], dup_jaccard=0.8,
           mega_additions=10000, behind_threshold=100, new_arrival_days=2, pareto_n=5)
MAIN = "a" * 40
BASE_TIME = datetime(2026, 10, 27, 12, tzinfo=timezone.utc)


class Clock(datetime):
    current = BASE_TIME

    @classmethod
    def now(cls, tz=None):
        return cls.current


def pr(number, **changes):
    row = dict(number=number, title="Feature %d" % number, user={"login": "offline"},
               created_at="2026-10-26T12:00:00Z", updated_at="2026-10-26T14:00:00Z",
               draft=False, head={"sha": "%040x" % number})
    row.update(changes)
    return row


class Response:
    status = 200

    def __init__(self, payload, remaining=500):
        self.payload = payload
        self.headers = {"x-ratelimit-remaining": str(remaining),
                        "x-ratelimit-resource": "core"}

    def __enter__(self):
        return self

    def __exit__(self, *args):
        return False

    def read(self):
        return json.dumps(self.payload).encode("utf-8")


class Transport:
    """Only urlopen is replaced; the candidate's real Gh handles all requests."""

    def __init__(self, prs, main=MAIN, override=None, files=None, mergeable=True):
        self.prs = prs
        self.main = main
        self.override = override
        self.files = files or {}
        self.mergeable = mergeable
        self.paths = []

    @property
    def deep(self):
        return [p for p in self.paths if "/pulls/" in p or "/compare/" in p]

    def __call__(self, request, timeout=60):
        path = urlsplit(request.full_url).path
        self.paths.append(path)
        if self.override:
            result = self.override(path)
            if result is not None:
                return result
        if path.endswith("/commits/main"):
            return Response({"sha": self.main})
        if path.endswith("/pulls"):
            return Response(self.prs)
        if "/pulls/" in path:
            n = int(path.split("/pulls/")[1].split("/")[0])
            if path.endswith("/files"):
                return Response(self.files.get(n, [
                    {"filename": "tests/p%d.hy" % n, "additions": 15, "deletions": 3}]))
            return Response({"mergeable": self.mergeable, "mergeable_state": "clean"})
        if "/compare/" in path:
            return Response({"behind_by": 2, "merge_base_commit": {"sha": "c" * 40},
                             "files": []})
        raise AssertionError("Unexpected offline request: %s" % path)


class CandidateTests(unittest.TestCase):
    def setUp(self):
        # Scoped mocks keep this suite offline without changing other suites.
        for target in ("socket.socket", "socket.getaddrinfo", "subprocess.Popen"):
            patcher = patch(target, side_effect=RuntimeError("Offline test forbids external IO"))
            patcher.start()
            self.addCleanup(patcher.stop)
        self.temp = tempfile.TemporaryDirectory(prefix="test-", dir=ROOT)
        self.addCleanup(self.temp.cleanup)
        self.out = Path(self.temp.name)
        Clock.current = BASE_TIME
        self.cfg = copy.deepcopy(CFG)
        self.sleeps = []
        for target, replacement in (
                ("datetime", Clock), ("log", lambda *args: None)):
            patcher = patch.object(bot, target, replacement)
            patcher.start()
            self.addCleanup(patcher.stop)
        patcher = patch.object(bot.time, "sleep", self.sleeps.append)
        patcher.start()
        self.addCleanup(patcher.stop)

    def scan(self, prs, *, jobs=1, cache=None, **transport_args):
        transport = Transport(prs, **transport_args)
        gh = bot.Gh("offline/repo", "offline-placeholder", min_remaining=150)
        args = SimpleNamespace(jobs=jobs, out=str(self.out), cache=cache)
        with patch.object(bot, "urlopen", transport):
            items, main = bot.collect(self.cfg, gh, args)
        return items, main, transport, gh

    def save_cache(self, items, main=MAIN, filename="TRIAGE.json"):
        path = self.out / filename
        path.write_text(json.dumps({"generated": "offline", "main": main,
                                    "items": items}), encoding="utf-8")
        return path

    def classify_render(self, items):
        items, clusters = bot.classify(self.cfg, items)
        board, pareto = bot.render_board(self.cfg, items, clusters, {}, "offline")
        return items, clusters, board, pareto

    def assert_unknown(self, row):
        for key in ("files", "additions", "deletions", "behind_by", "tests_included"):
            self.assertIsNone(row[key], key)

    def test_draft_fast_path_has_unknown_measurements_and_renders(self):
        items, _, transport, _ = self.scan([pr(1, draft=True)])
        self.assertEqual(transport.deep, [])
        self.assert_unknown(items[0])
        items, _, board, pareto = self.classify_render(items)
        self.assertEqual(items[0]["bucket"], "PARKED-DRAFT")
        self.assertEqual(pareto, [])
        self.assertIn("| ? | ? | ? | ? |", board)

    def test_fresh_complete_schema_and_classification(self):
        items, _, transport, _ = self.scan([pr(1)])
        self.assertEqual(len(transport.deep), 4)
        self.assertEqual(items[0]["files"], 1)
        self.assertEqual(items[0]["additions"], 15)
        self.assertEqual(items[0]["behind_by"], 2)
        self.assertTrue(items[0]["tests_included"])
        items, _, board, pareto = self.classify_render(items)
        self.assertEqual(items[0]["bucket"], "LAND-CANDIDATE")
        self.assertEqual(items[0]["score"], 8)
        self.assertEqual(len(pareto), 1)
        self.assertIn("| +15 | 1 | Y | 2 | 8 |", board)
        self.assertNotIn("Incomplete scans", board)

    def test_cache_reuses_only_measurements_not_classification(self):
        items, _, _, _ = self.scan([pr(1)])
        items[0].update(dup_of=999, dup_members=[998], bucket_new=True,
                        merge_gate="MERGE-CLEAN", labels=["stale"], bucket="DUP", score=-1)
        self.save_cache(items)
        rows, _, transport, _ = self.scan([pr(1)])
        self.assertEqual(transport.deep, [])
        for key in ("dup_of", "dup_members", "bucket_new", "merge_gate",
                    "labels", "bucket", "score"):
            self.assertNotIn(key, rows[0])

    def test_degraded_cache_recovers_with_healthy_quota(self):
        def limited(path):
            if path.endswith("/pulls/1"):
                return Response({}, remaining=149)
        items, _, _, _ = self.scan([pr(1)], override=limited)
        self.classify_render(items)
        self.save_cache(items)
        rows, _, transport, _ = self.scan([pr(1)])
        self.assertTrue(transport.deep)
        self.assertNotIn("degraded", rows[0])
        self.assertTrue(rows[0]["mergeable"])
        self.assertEqual(rows[0]["files"], 1)

    def test_unresolved_mergeability_is_retried(self):
        items, _, _, _ = self.scan([pr(1)], mergeable=None)
        self.save_cache(items)
        rows, _, transport, _ = self.scan([pr(1)])
        self.assertTrue(transport.deep)
        self.assertTrue(rows[0]["mergeable"])

    def test_cache_keys_invalidate_independently(self):
        base, _, _, _ = self.scan([pr(1)])
        for key in ("head", "updated_at", "draft", "main"):
            with self.subTest(key=key):
                row = copy.deepcopy(base[0])
                current = pr(1)
                main = MAIN
                if key == "head":
                    current["head"]["sha"] = "d" * 40
                elif key == "updated_at":
                    current["updated_at"] = "2026-10-27T00:00:00Z"
                elif key == "draft":
                    row["draft"] = True
                else:
                    main = "e" * 40
                self.save_cache([row])
                rows, _, transport, _ = self.scan([current], main=main)
                self.assertTrue(transport.deep)
                self.assertEqual(rows[0]["head_sha"], current["head"]["sha"][:12])

    def test_ready_to_draft_skips_deep_calls(self):
        items, _, _, _ = self.scan([pr(1)])
        self.save_cache(items)
        rows, _, transport, _ = self.scan([pr(1, draft=True)])
        self.assertEqual(transport.deep, [])
        self.assert_unknown(rows[0])

    def test_incomplete_or_malformed_cache_refetches(self):
        base, _, _, _ = self.scan([pr(1)])
        corruptions = [
            ("paths_sample", None), ("files", "1"), ("files", True),
            ("behind_by", -1), ("tests_included", None),
            ("overlap_risk", [None]), ("mergeable_state", {}),
        ]
        for key, value in corruptions:
            with self.subTest(key=key, value=value):
                row = copy.deepcopy(base[0])
                row[key] = value
                self.save_cache([row])
                items, _, transport, _ = self.scan([pr(1)])
                self.assertTrue(transport.deep)
                self.classify_render(items)
        row = copy.deepcopy(base[0])
        del row["paths_sample"]
        self.save_cache([row])
        items, _, transport, _ = self.scan([pr(1)])
        self.assertTrue(transport.deep)
        self.classify_render(items)

    def test_invalid_json_falls_back_to_fresh_collection(self):
        (self.out / "TRIAGE.json").write_text("{broken", encoding="utf-8")
        items, _, transport, _ = self.scan([pr(1)])
        self.assertTrue(transport.deep)
        self.classify_render(items)

    def test_explicit_cache_path_is_supported(self):
        items, _, _, _ = self.scan([pr(1)])
        path = self.save_cache(items, filename="explicit.json")
        _, _, transport, _ = self.scan([pr(1)], cache=str(path))
        self.assertEqual(transport.deep, [])

    def test_scan_command_cache_survives_a_new_output_directory(self):
        args = SimpleNamespace(jobs=1, out=str(self.out), cache=None, verify=None)
        cfg = dict(self.cfg, repo="offline/repo")
        transports = []
        for run in range(2):
            transport = Transport([pr(1)])
            transports.append(transport)
            with patch.object(bot, "load_cfg", return_value=cfg), \
                    patch.object(bot, "load_token", return_value="offline-placeholder"), \
                    patch.object(bot, "urlopen", transport):
                bot.cmd_scan(args)
            output = Path(args.out)
            data = json.loads((output / "TRIAGE.json").read_text())
            self.assertEqual(data["main"], MAIN)
            self.assertEqual(data["items"][0]["bucket"], "LAND-CANDIDATE")
            self.assertTrue((output / "TRIAGE.md").is_file())
            if run == 0:
                # Simulate restoring only the snapshot on a fresh runner.
                restored = self.out / "next-run"
                restored.mkdir()
                (restored / "TRIAGE.json").write_bytes((output / "TRIAGE.json").read_bytes())
                args.out = str(restored)
        self.assertEqual(len(transports[0].deep), 4)
        self.assertEqual(transports[1].deep, [])

    def test_shepherd_sync_labels_accepts_nulls_without_labeling_incomplete_rows(self):
        def limited(path):
            if path.endswith("/pulls/3"):
                return Response({}, remaining=149)
        items, _, _, _ = self.scan([pr(1), pr(2, draft=True), pr(3)], override=limited)
        rows, _, _, _ = self.classify_render(items)
        self.assert_unknown(next(row for row in rows if row["number"] == 3))
        path = self.save_cache(rows)
        gh = SimpleNamespace(repo="offline/repo", calls=0, remaining=500)
        posted = []
        gh.post = lambda endpoint, payload: posted.append((endpoint, payload))
        with patch.object(shepherd, "tb", bot), \
                patch.object(shepherd, "load_cfg", return_value={"repo": gh.repo}), \
                patch.object(shepherd, "ensure_palette") as palette, \
                patch.object(bot, "load_token", return_value="offline-placeholder"), \
                patch.object(bot, "Gh", return_value=gh):
            shepherd.cmd_sync_labels(SimpleNamespace(from_scan=str(path), dry=False))
        palette.assert_called_once()
        self.assertEqual(posted, [("/repos/offline/repo/issues/1/labels",
                                   {"labels": ["bot:land-candidate"]})])

    def duplicate_files(self):
        return {n: [{"filename": "tests/shared.hy", "additions": 15, "deletions": 0}]
                for n in (1, 2)}

    def test_closed_primary_clears_duplicate_and_renders_survivor(self):
        items, _, _, _ = self.scan([pr(1), pr(2)], files=self.duplicate_files())
        self.classify_render(items)
        self.save_cache(items)
        rows, _, _, _ = self.scan([pr(2)])
        rows, clusters, board, _ = self.classify_render(rows)
        self.assertEqual(clusters, [])
        self.assertNotIn("dup_of", rows[0])
        self.assertNotIn("triage:duplicate", rows[0]["labels"])
        self.assertEqual(rows[0]["bucket"], "LAND-CANDIDATE")
        self.assertIn("| #2 |", board)

    def test_duplicate_members_do_not_accumulate(self):
        items, _, _, _ = self.scan([pr(1), pr(2)], files=self.duplicate_files())
        for _ in range(3):
            items, clusters, _, _ = self.classify_render(items)
            self.assertEqual(clusters, [[1, 2]])
            self.assertEqual(next(i for i in items if i["number"] == 1)["dup_members"], [2])
            self.save_cache(items)
            items, _, transport, _ = self.scan([pr(1), pr(2)])
            self.assertEqual(transport.deep, [])
        self.classify_render(items)
        self.classify_render(items)
        self.assertEqual(next(i for i in items if i["number"] == 1)["dup_members"], [2])

    def test_changed_primary_clears_old_duplicate_link(self):
        items, _, _, _ = self.scan([pr(1), pr(2)], files=self.duplicate_files())
        self.classify_render(items)
        self.save_cache(items)
        rows, _, _, _ = self.scan([pr(1, head={"sha": "f" * 40}), pr(2)])
        rows, clusters, _, _ = self.classify_render(rows)
        self.assertEqual(clusters, [])
        self.assertTrue(all("dup_of" not in row for row in rows))

    def test_cache_refreshes_age_idle_and_new_arrival(self):
        items, _, _, _ = self.scan([pr(1)])
        self.classify_render(items)
        self.save_cache(items)
        Clock.current = BASE_TIME + timedelta(days=10)
        rows, _, transport, _ = self.scan([pr(1)])
        rows, _, board, _ = self.classify_render(rows)
        self.assertEqual(transport.deep, [])
        self.assertEqual(rows[0]["age_days"], 11.0)
        self.assertEqual(rows[0]["idle_days"], 10.9)
        self.assertNotIn("triage:new", rows[0]["labels"])
        self.assertNotIn("bucket_new", rows[0])
        self.assertIn("| 11.0d |", board)
        rows[0]["bucket_new"] = True
        self.classify_render(rows)
        self.assertNotIn("bucket_new", rows[0])

    def test_real_gh_low_quota_stops_queued_requests(self):
        def limited(path):
            if "/pulls/" in path:
                return Response({}, remaining=149)
        items, _, transport, gh = self.scan([pr(n) for n in range(1, 5)], override=limited)
        self.assertEqual(len(transport.deep), 1)
        self.assertEqual(len(items), 4)
        self.assertTrue(all(row.get("degraded") for row in items))
        self.assertEqual(self.sleeps, [])
        with patch.object(bot, "urlopen", transport), self.assertRaises(bot.RateGuard):
            gh.get("/repos/offline/repo/pulls/99")
        self.assertEqual(len(transport.deep), 1)

    def test_real_gh_concurrent_inflight_requests_are_bounded(self):
        barrier = Barrier(3)
        def limited(path):
            if "/pulls/" in path:
                barrier.wait(timeout=5)
                return Response({}, remaining=149)
        items, _, transport, _ = self.scan([pr(n) for n in range(1, 13)],
                                           jobs=3, override=limited)
        self.assertEqual(len(transport.deep), 3)
        self.assertEqual(len(items), 12)
        self.assertTrue(all(row.get("degraded") for row in items))

    def test_real_gh_http_rate_limits_latch_without_backoff(self):
        for status, body, headers in (
                (403, b"API rate limit exceeded", {}),
                (403, b"secondary rate limit", {}),
                (403, b"quota exhausted", {"x-ratelimit-remaining": "0"}),
                (429, b"too many requests", {})):
            with self.subTest(status=status, body=body):
                def limited(path):
                    if "/pulls/" in path:
                        raise HTTPError("https://offline.invalid", status, "limited",
                                        headers, io.BytesIO(body))
                items, _, transport, _ = self.scan([pr(n) for n in range(1, 5)],
                                                   override=limited)
                self.assertEqual(len(transport.deep), 1)
                self.assertTrue(all(row.get("degraded") for row in items))
                self.assertEqual(self.sleeps, [])

    def test_permission_error_still_propagates(self):
        def denied(path):
            if "/pulls/" in path:
                raise HTTPError("https://offline.invalid", 403, "Forbidden", {},
                                io.BytesIO(b"Resource not accessible by integration"))
        with self.assertRaisesRegex(RuntimeError, "403"):
            self.scan([pr(1)], override=denied)
        self.assertEqual(self.sleeps, [])

    def test_rate_limit_before_pr_snapshot_still_propagates(self):
        for stage in ("pulls", "main"):
            with self.subTest(stage=stage):
                def limited(path):
                    matches = path.endswith("/pulls") if stage == "pulls" else path.endswith("/commits/main")
                    if matches:
                        return Response({}, remaining=149)
                with self.assertRaises(bot.RateGuard):
                    self.scan([pr(1)], override=limited)

    def test_existing_conflict_stale_and_recon_buckets(self):
        rows, _, _, _ = self.scan([pr(1)], mergeable=False)
        self.assertEqual(self.classify_render(rows)[0][0]["bucket"], "CONFLICTS")
        def stale(path):
            if "/compare/" in path:
                return Response({"behind_by": 200, "merge_base_commit": {"sha": "c" * 40}})
        rows, _, _, _ = self.scan([pr(1)], override=stale)
        self.assertEqual(self.classify_render(rows)[0][0]["bucket"], "STALE-BASE")
        rows, _, _, _ = self.scan([pr(1)], mergeable=None, files={
            1: [{"filename": "src/parser.hy", "additions": 15, "deletions": 3}]})
        self.assertEqual(self.classify_render(rows)[0][0]["bucket"], "RECON")

    def test_transient_server_retry_is_preserved(self):
        attempts = []
        def transient(path):
            if path.endswith("/pulls/1"):
                attempts.append(path)
                if len(attempts) == 1:
                    raise HTTPError("https://offline.invalid", 503, "Unavailable", {},
                                    io.BytesIO(b"try later"))
        items, _, _, _ = self.scan([pr(1)], override=transient)
        self.assertEqual(len(attempts), 2)
        self.assertEqual(self.sleeps, [2])
        self.assertNotIn("degraded", items[0])

    def test_partial_fetch_rate_limit_does_not_publish_zero_measurements(self):
        for stage in ("files", "compare"):
            with self.subTest(stage=stage):
                def limited(path):
                    matches = path.endswith("/files") if stage == "files" else "/compare/" in path
                    if matches:
                        return Response({}, remaining=149)
                items, _, _, _ = self.scan([pr(1)], override=limited)
                self.assert_unknown(items[0])
                rows, _, board, pareto = self.classify_render(items)
                self.assertEqual(rows[0]["bucket"], "INCOMPLETE")
                self.assertEqual(rows[0]["score"], 0)
                self.assertEqual(pareto, [])
                self.assertIn("Incomplete scans", board)
                self.assertIn("rate limited; PR details unknown", board)
                self.assertIn("| ? | ? | ? | ? |", board)

    def test_degraded_wildcard_keeps_hold_and_discloses_unknowns(self):
        self.cfg["wildcard_holds"] = [1]
        def limited(path):
            if "/pulls/" in path:
                return Response({}, remaining=149)
        items, _, _, _ = self.scan([pr(1)], override=limited)
        rows, _, board, pareto = self.classify_render(items)
        self.assertEqual(rows[0]["bucket"], "PARKED-WILDCARD")
        self.assertTrue(rows[0]["action"].startswith("PARK"))
        self.assertIn("Incomplete scans", board)
        self.assertEqual(pareto, [])

    def test_mixed_normal_draft_and_degraded_schema(self):
        def limited(path):
            if path.endswith("/pulls/3"):
                return Response({}, remaining=149)
        items, _, _, _ = self.scan([pr(1), pr(2, draft=True), pr(3)], override=limited)
        rows, _, board, _ = self.classify_render(items)
        buckets = {row["number"]: row["bucket"] for row in rows}
        self.assertEqual(buckets, {1: "LAND-CANDIDATE", 2: "PARKED-DRAFT", 3: "INCOMPLETE"})
        self.assertIn("| +15 | 1 | Y | 2 |", board)
        self.assertIn("| ? | ? | ? | ? |", board)
        json.dumps(rows, allow_nan=False)


if __name__ == "__main__":
    unittest.main(verbosity=2)
