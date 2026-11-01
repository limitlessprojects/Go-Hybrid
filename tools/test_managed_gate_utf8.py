"""Real-pipe regressions for managed metadata on non-UTF-8 hosts."""

import contextlib
import importlib.util
import io
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest
from unittest.mock import patch


SCRIPT = Path(__file__).with_name("managed_gate.py")
SPEC = importlib.util.spec_from_file_location("managed_gate_utf8", SCRIPT)
gate = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(gate)
REAL_RUN = subprocess.run


class ManagedGateUtf8Tests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.source = self.root / "program.hy"
        self.source.write_text('fn main() { managed("\u6d4b\u8bd5", "other") {} }\n', encoding="utf-8")
        self.checker = self.root / "checker.py"
        self.block = {"reason": "\u6d4b\u8bd5", "category": "other"}
        self.checker.write_text(
            "import json, sys\n"
            "report = {'summary': {'managed': {'managed_blocks': 1, 'blocks': "
            + repr([self.block]) + "}}}\n"
            "sys.stdout.buffer.write(json.dumps(report, ensure_ascii=False).encode('utf-8'))\n",
            encoding="utf-8",
        )

    def run_with_legacy_locale(self, command, **kwargs):
        if kwargs.get("text") or kwargs.get("universal_newlines"):
            kwargs.setdefault("encoding", "cp936")
        return REAL_RUN([sys.executable, str(self.checker), *command[1:]], **kwargs)

    def test_unicode_reason_is_decoded_as_checker_utf8(self):
        with patch.object(gate.subprocess, "run", self.run_with_legacy_locale):
            actual = gate.audit(str(self.checker), str(self.source))
        self.assertEqual(actual, {str(self.source) + ":1": self.block})

    def test_existing_unicode_baseline_does_not_report_changed(self):
        baseline = self.root / "baseline.json"
        original = json.dumps({"version": 1, "blocks": {
            str(self.source) + ":1": self.block,
        }}).encode("utf-8")
        baseline.write_bytes(original)
        output = io.StringIO()
        with patch.object(sys, "argv", [str(SCRIPT), str(self.checker),
                                       str(baseline), str(self.source)]), \
                patch.object(gate.subprocess, "run", self.run_with_legacy_locale), \
                contextlib.redirect_stdout(output):
            gate.main()
        self.assertIn("managed gate: PASS", output.getvalue())
        self.assertNotIn("CHANGED", output.getvalue())
        self.assertEqual(baseline.read_bytes(), original)

    def test_rebaseline_preserves_unicode_reason(self):
        baseline = self.root / "baseline.json"
        with patch.object(sys, "argv", [str(SCRIPT), "--rebaseline", str(self.checker),
                                       str(baseline), str(self.source)]), \
                patch.object(gate.subprocess, "run", self.run_with_legacy_locale), \
                contextlib.redirect_stdout(io.StringIO()):
            gate.main()
        self.assertEqual(json.loads(baseline.read_text(encoding="utf-8"))["blocks"],
                         {str(self.source) + ":1": self.block})


if __name__ == "__main__":
    unittest.main()
