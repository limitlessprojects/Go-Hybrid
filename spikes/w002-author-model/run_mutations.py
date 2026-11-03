"""Run existing semantic witnesses against two deliberately defective models."""

from pathlib import Path
import subprocess
import sys
import tempfile


def main():
    root = Path(__file__).resolve().parent
    source = (root / "model.py").read_text(encoding="utf-8")
    mutants = [
        ("remove-live-borrow-transfer-guard",
         'node.kind == "own" and self.borrowed(node.resource)',
         'False', "test_live_borrow_blocks_move"),
        ("remove-duplicate-capture-guard",
         'len(names) != len(set(names))',
         'False', "test_duplicate_capture_rejects_atomically"),
    ]
    for label, old, new, witness in mutants:
        if source.count(old) != 1:
            raise RuntimeError(f"mutation target changed: {label}")
        with tempfile.TemporaryDirectory(prefix="w002-mutant-") as temp:
            directory = Path(temp)
            (directory / "model.py").write_text(source.replace(old, new), encoding="utf-8")
            (directory / "test_model.py").write_text(
                (root / "test_model.py").read_text(encoding="utf-8"), encoding="utf-8")
            result = subprocess.run(
                [sys.executable, "-m", "unittest",
                 f"test_model.CapabilityTests.{witness}"],
                cwd=directory, capture_output=True, text=True, timeout=30,
            )
            output = result.stdout + result.stderr
            if (result.returncode != 1 or "FAIL:" not in output
                    or "Rejected not raised" not in output):
                raise RuntimeError(f"mutant not caught by semantic witness: {label}\n{output}")
            print(f"CAUGHT {label}: {witness}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
