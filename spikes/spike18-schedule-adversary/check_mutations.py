#!/usr/bin/env python3
"""Check that the independent oracles detect three deliberately broken versions.

Only this spike is copied to temporary directories. No repository file is
modified; all subprocesses are local Go tests with no third-party dependencies.
An unexpected failure (e.g. compilation) is not counted as killing a mutant.
"""

from pathlib import Path
import os
import shutil
import subprocess
import tempfile


MUTANTS = (
    (
        "self_loop_consumes_future_output",
        "certificate.go",
        "if occupancy[i] < e.Consume {",
        "if occupancy[i] < e.Consume && e.From != a {",
        "^TestExhaustiveWitnesses$",
        "oracle false, replay <nil>",
    ),
    (
        "unchecked_payload_budget",
        "certificate.go",
        "if payload != c.PayloadBytes {",
        "if false {",
        "^TestMutatedWitnesses/(underclaimed_bytes|overclaimed_bytes|wrapped_bytes)$",
        "mutated certificate accepted",
    ),
    (
        "missing_runtime_underflow_guard",
        "emit.go",
        "if k.n%d <= 0 {",
        "if false && k.n%d <= 0 {",
        "^TestGeneratedKernels/pipeline$",
        "panic=<nil>; want FIFO underflow",
    ),
)


def main():
    source = Path(__file__).resolve().parent
    env = dict(os.environ, GOTOOLCHAIN="local", GOWORK="off")
    for name, filename, old, new, pattern, diagnostic in MUTANTS:
        with tempfile.TemporaryDirectory(prefix="w018-mutant-") as tmp:
            target = Path(tmp) / "spike"
            shutil.copytree(source, target, ignore=shutil.ignore_patterns("__pycache__"))
            path = target / filename
            text = path.read_text()
            if text.count(old) != 1:
                raise SystemExit(f"mutation {name}: expected exactly one source location")
            path.write_text(text.replace(old, new))
            command = ["go", "test", "-v", "-count=1", "-run", pattern, "./..."]
            print(f"MUTANT {name}: {command}", flush=True)
            result = subprocess.run(
                command, cwd=target, env=env, stdout=subprocess.PIPE,
                stderr=subprocess.STDOUT, text=True, timeout=180,
            )
            print(result.stdout, end="", flush=True)
            if result.returncode != 1 or diagnostic not in result.stdout:
                raise SystemExit(f"mutation {name}: expected specific test failure was absent")
            print(f"KILLED {name} (expected test failure)", flush=True)
    print("PASS: all 3 deliberately broken implementations detected", flush=True)


if __name__ == "__main__":
    main()
