#!/usr/bin/env python3
"""Offline unit tests for tools/managed_gate.py: method-call disambiguation in
scan_managed_lines, reported block reason/tickets validation in audit(), and
baseline block entry schema validation in main()."""

import contextlib
import io
import json
import pathlib
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent.parent))
import managed_gate as gate  # noqa: E402


class ManagedGateValidationTests(unittest.TestCase):
    def test_scan_managed_lines_ignores_dot_method_calls(self):
        src = "\n".join([
            "fn main() {",
            "  cfg.managed(\"not-a-hatch\", \"other\")",
            "  self.managed (1, 2)",
            "  managed(\"real-hatch\", \"mutex\") {}",
            "}",
        ])
        self.assertEqual(gate.scan_managed_lines(src), [4])

    def test_audit_rejects_missing_or_empty_reason_and_bad_tickets(self):
        bad_blocks = [
            {"category": "other"},
            {"category": "other", "reason": ""},
            {"category": "other", "reason": "   "},
            {"category": "other", "reason": 123},
            {"category": "other", "reason": "ok", "tickets": "not-a-list"},
            {"category": "other", "reason": "ok", "tickets": [123]},
        ]
        with tempfile.TemporaryDirectory() as td:
            prog = pathlib.Path(td) / "prog.hy"
            prog.write_text('managed("ok", "other") {}\n', encoding="utf-8")
            for b in bad_blocks:
                with self.subTest(block=b):
                    payload = {"summary": {"managed": {"managed_blocks": 1, "blocks": [b]}}}
                    res = subprocess.CompletedProcess([], 0, json.dumps(payload), "")
                    out = io.StringIO()
                    with patch.object(gate.subprocess, "run", return_value=res), \
                         contextlib.redirect_stdout(out), \
                         self.assertRaises(SystemExit) as cm:
                        gate.audit("hyb", str(prog))
                    self.assertEqual(cm.exception.code, 2)
                    self.assertIn("GATE ERROR", out.getvalue())

    def test_main_rejects_malformed_baseline_entries_before_audit(self):
        bad_baselines = [
            {"version": 1, "blocks": {"no_colon_key": {"reason": "x", "category": "other"}}},
            {"version": 1, "blocks": {"prog.hy:notanint": {"reason": "x", "category": "other"}}},
            {"version": 1, "blocks": {":10": {"reason": "x", "category": "other"}}},
            {"version": 1, "blocks": {"prog.hy:1": "not-a-dict"}},
            {"version": 1, "blocks": {"prog.hy:1": {"category": "other"}}},
            {"version": 1, "blocks": {"prog.hy:1": {"reason": "   ", "category": "other"}}},
            {"version": 1, "blocks": {"prog.hy:1": {"reason": "ok", "category": "bogus"}}},
        ]
        with tempfile.TemporaryDirectory() as td:
            root = pathlib.Path(td)
            prog = root / "prog.hy"
            prog.write_text("fn main() {}\n", encoding="utf-8")
            bpath = root / "baseline.json"
            for doc in bad_baselines:
                for rebaseline in (False, True):
                    with self.subTest(doc=doc, rebaseline=rebaseline):
                        raw = json.dumps(doc).encode("utf-8")
                        bpath.write_bytes(raw)
                        argv = ["managed_gate.py"] + (["--rebaseline"] if rebaseline else [])
                        argv += ["hyb", str(bpath), str(prog)]
                        out = io.StringIO()
                        with patch.object(gate.sys, "argv", argv), \
                             patch.object(gate, "audit") as mock_audit, \
                             contextlib.redirect_stdout(out), \
                             self.assertRaises(SystemExit) as cm:
                            gate.main()
                        self.assertEqual(cm.exception.code, 2)
                        self.assertIn("GATE ERROR", out.getvalue())
                        mock_audit.assert_not_called()
                        self.assertEqual(bpath.read_bytes(), raw)


if __name__ == "__main__":
    unittest.main()
