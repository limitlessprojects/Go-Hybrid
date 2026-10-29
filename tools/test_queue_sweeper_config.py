"""The destructive sweep must stop if its protection config is unavailable."""

import contextlib
import io
import json
import os
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parent))
import queue_sweeper as sweeper


class ProtectionConfigTests(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self.tmp.cleanup)
        self.path = Path(self.tmp.name) / "triage_config.json"

    def load(self, value):
        self.path.write_text(json.dumps(value), encoding="utf-8")
        with patch("builtins.open", return_value=self.path.open(encoding="utf-8")):
            return sweeper.config_protected_numbers()

    def test_unions_both_protection_lists(self):
        self.assertEqual(self.load({"wildcard_holds": [1, 2],
                                    "drafts_parked": [2, 3]}), {1, 2, 3})

    def test_explicitly_empty_lists_are_valid(self):
        self.assertEqual(self.load({"wildcard_holds": [], "drafts_parked": []}), set())

    def test_missing_file_is_an_error(self):
        with patch("builtins.open", side_effect=FileNotFoundError("missing config")):
            with self.assertRaisesRegex(ValueError, "protection config"):
                sweeper.config_protected_numbers()

    def test_bad_json_and_bad_utf8_are_errors(self):
        for raw in (b"{broken", b"\xff"):
            with self.subTest(raw=raw):
                self.path.write_bytes(raw)
                with patch("builtins.open", return_value=self.path.open(encoding="utf-8")):
                    with self.assertRaisesRegex(ValueError, "protection config"):
                        sweeper.config_protected_numbers()

    def test_invalid_schema_is_an_error(self):
        cases = [[], None, {}, {"wildcard_holds": []},
                 {"wildcard_holds": "123", "drafts_parked": []},
                 {"wildcard_holds": [], "drafts_parked": None}]
        for value in ("7", True, False, 0, -1, 1.5, [], {}):
            cases.append({"wildcard_holds": [value], "drafts_parked": []})
            cases.append({"wildcard_holds": [7], "drafts_parked": [value]})
        for value in cases:
            with self.subTest(value=value):
                with self.assertRaisesRegex(ValueError, "protection config"):
                    self.load(value)

    def test_default_config_does_not_depend_on_working_directory(self):
        source = Path(sweeper.__file__).with_name("triage_config.json")
        config = json.loads(source.read_text(encoding="utf-8"))
        expected = set(config["wildcard_holds"] + config["drafts_parked"])
        previous = os.getcwd()
        try:
            os.chdir(self.tmp.name)
            self.assertEqual(sweeper.config_protected_numbers(), expected)
        finally:
            os.chdir(previous)

    def test_bad_config_stops_main_before_any_github_call(self):
        with patch.object(sys, "argv", ["queue_sweeper.py"]), \
                patch.object(sweeper, "TOK", "offline-test-token"), \
                patch("builtins.open", side_effect=FileNotFoundError("missing config")), \
                patch.object(sweeper, "api", return_value=(404, None)) as api, \
                contextlib.redirect_stdout(io.StringIO()), \
                contextlib.redirect_stderr(io.StringIO()) as stderr:
            with self.assertRaises(SystemExit) as raised:
                sweeper.main()
            self.assertEqual(raised.exception.code, 1)
            self.assertIn("protection config", stderr.getvalue())
            api.assert_not_called()


if __name__ == "__main__":
    unittest.main()
