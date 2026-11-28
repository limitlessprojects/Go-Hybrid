#!/usr/bin/env python3
"""Run the #833 experiment, including a separately verified expected race."""

import os
from pathlib import Path
import subprocess
import sys


def main():
    work = Path(__file__).resolve().parent / "work"
    env = os.environ.copy()
    env.pop("HYB_ATOMIC_BOX_NEGATIVE", None)
    env["GOTOOLCHAIN"] = "local"

    def run(args, *, negative=False):
        command = ["go", *args]
        print("$ " + " ".join(command), flush=True)
        child_env = env.copy()
        if negative:
            child_env["HYB_ATOMIC_BOX_NEGATIVE"] = "1"
            child_env["GORACE"] = "halt_on_error=1"
        result = subprocess.run(
            command, cwd=work, env=child_env, text=True,
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT, timeout=600,
        )
        print(result.stdout, end="", flush=True)
        if negative:
            if result.returncode != 1 or "WARNING: DATA RACE" not in result.stdout:
                raise RuntimeError("negative control did not produce the expected Go race failure")
            print("EXPECTED FAILURE: raw Load/Next access reported a real pointee race", flush=True)
        elif result.returncode:
            raise RuntimeError(f"command failed with exit {result.returncode}")

    run(["version"])
    run(["vet", "./..."])
    run(["test", "-race", "-count=1", "-cpu=8,16,32", "-timeout=5m", "-v", "./..."])
    # Stress the small semantic/progress controls 3000 times. The million-
    # exchange test is separately run at all three CPU settings above.
    run(["test", "-race", "-count=1000", "-cpu=8,16,32", "-timeout=5m",
         "-run=Test(CAS|Self|Treiber|Take)", "./..."])
    run(["test", "-race", "-count=1", "-timeout=2m",
         "-run=^TestUncheckedTreiberLoadRace$", "-v", "./..."], negative=True)
    run(["test", "-run=^$", "-bench=^BenchmarkSwap$", "-benchmem", "-cpu=1",
         "-benchtime=1s", "-count=5", "./..."])
    print("All experiment checks passed; the raw-Load negative control failed as expected.", flush=True)


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, subprocess.TimeoutExpired) as error:
        print(f"EXPERIMENT FAILED: {error}", file=sys.stderr)
        sys.exit(1)
