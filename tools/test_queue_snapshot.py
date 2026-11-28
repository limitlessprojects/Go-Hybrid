"""Offline output-path regressions for queue_snapshot.py (issue #4906).

Run the real script with one fixture GraphQL response and a synthetic token.
No GitHub access or real credentials are needed.
"""
import contextlib
import io
import json
import os
from pathlib import Path
import runpy
import tempfile
import unittest
from unittest.mock import patch


SCRIPT = Path(__file__).with_name("queue_snapshot.py").resolve()
PR = {
    "number": 42,
    "title": "Offline snapshot fixture",
    "createdAt": "2026-11-01T00:00:00Z",
    "updatedAt": "2026-11-02T00:00:00Z",
    "isDraft": False,
    "author": {"login": "fixture"},
    "mergeable": "UNKNOWN",
    "mergeStateStatus": "UNKNOWN",
    "labels": {"nodes": []},
    "comments": {"totalCount": 0},
    "commits": {"nodes": []},
}


class SnapshotOutputPathTests(unittest.TestCase):
    def run_snapshot(self, output_kind, nodes):
        with tempfile.TemporaryDirectory(prefix="queue snapshot ") as directory:
            base = Path(directory)
            destinations = {
                "bare": Path("snapshot.json"),
                "nested": Path("reports") / "nested" / "snapshot.json",
                "absolute": base / "absolute" / "snapshot.json",
                "spaces": Path("report folder") / "snapshot file.json",
                "default": base / "state" / "queue_snapshot.json",
            }
            destination = destinations[output_kind]
            environment = {"GH_TOKEN": "offline-fixture"}
            if output_kind == "default":
                environment["TRIAGE_STATE_DIR"] = str(base / "state")
            else:
                environment["SURVEY_OUT"] = str(destination)
            response = {
                "data": {"repository": {"pullRequests": {
                    "nodes": nodes,
                    "pageInfo": {"hasNextPage": False, "endCursor": None},
                }}}
            }

            def offline_urlopen(request, **kwargs):
                self.assertEqual(request.full_url, "https://api.github.com/graphql")
                self.assertEqual(request.get_method(), "POST")
                self.assertEqual(
                    request.get_header("Authorization"), "Bearer offline-fixture")
                self.assertEqual(
                    json.loads(request.data)["variables"], {"cursor": None})
                return io.BytesIO(json.dumps(response).encode("utf-8"))

            previous_directory = Path.cwd()
            try:
                os.chdir(base)
                with patch.dict(os.environ, environment, clear=True), \
                     patch("urllib.request.urlopen", side_effect=offline_urlopen) as request, \
                     patch("time.sleep", side_effect=AssertionError("unexpected retry")), \
                     contextlib.redirect_stdout(io.StringIO()):
                    runpy.run_path(str(SCRIPT), run_name="__main__")
                self.assertEqual(request.call_count, 1)
                target = destination if destination.is_absolute() else base / destination
                self.assertTrue(target.is_file())
                result = json.loads(target.read_text(encoding="utf-8"))
                self.assertEqual(len(result), len(nodes))
                for actual, expected in zip(result, nodes):
                    for key, value in expected.items():
                        self.assertEqual(actual[key], value)
                    self.assertEqual(actual["_ci_rollup"], "NONE")
                    self.assertEqual(actual["_labels"], [])
            finally:
                os.chdir(previous_directory)

    def check_output(self, kind):
        for nodes in ([], [PR]):
            with self.subTest(nodes=len(nodes)):
                self.run_snapshot(kind, nodes)

    def test_bare_filename(self):
        self.check_output("bare")

    def test_nested_relative_filename(self):
        self.check_output("nested")

    def test_absolute_filename(self):
        self.check_output("absolute")

    def test_relative_path_with_spaces(self):
        self.check_output("spaces")

    def test_default_state_directory(self):
        self.check_output("default")


if __name__ == "__main__":
    unittest.main()
