#!/usr/bin/env python3
"""Paired native-Go slice benchmark; protocol is in RESULTS.md."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import re
import shutil
import statistics
import subprocess
import time

HERE = Path(__file__).resolve().parent
SAMPLES = 11
EXPECTED = {"TwoIndex", "Prefix", "Suffix", "Whole", "Array", "Pointer", "Nested"}
LINE = re.compile(r"^BenchmarkSlice/(\w+)\s+\d+\s+([\d.]+) ns/op\s+(\d+) B/op\s+(\d+) allocs/op$", re.M)


def run(command, cwd, log, env):
    start = time.monotonic()
    proc = subprocess.run([str(x) for x in command], cwd=cwd, env=env,
                          stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
                          text=True, timeout=600)
    log.write_text("$ " + " ".join(str(x) for x in command) + "\n" + proc.stdout
                   + f"\nexit={proc.returncode} elapsed={time.monotonic() - start:.6f}s\n")
    if proc.returncode:
        raise RuntimeError(f"command failed; see {log}")
    return proc.stdout


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--patched", type=Path, required=True)
    parser.add_argument("--out", type=Path, required=True)
    args = parser.parse_args()
    out = args.out.resolve()
    out.mkdir(parents=True, exist_ok=False)
    env = dict(os.environ, GOMAXPROCS="1", GOTOOLCHAIN="local")
    metadata = {"platform": platform.platform(), "machine": platform.machine(),
                "samples": SAMPLES, "benchtime": "500ms", "cpus": 1,
                "threshold": 1.05, "start_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                "protocol_sha256": hashlib.sha256((HERE / "RESULTS.md").read_bytes()).hexdigest(),
                "compilers": {k: {"path": str(v.resolve()), "sha256": hashlib.sha256(v.read_bytes()).hexdigest()}
                              for k, v in [("main", args.baseline), ("patched", args.patched)]}}
    (out / "metadata.json").write_text(json.dumps(metadata, indent=2) + "\n")
    if Path("/proc/cpuinfo").exists():
        shutil.copyfile("/proc/cpuinfo", out / "cpuinfo.txt")
    run(["go", "version"], HERE, out / "go-version.log", env)
    run(["rustc", "-vV"], HERE, out / "rust-version.log", env)
    run(["git", "rev-parse", "HEAD"], HERE, out / "checkout.log", env)
    for label, compiler in [("main", args.baseline), ("patched", args.patched)]:
        directory = out / label
        directory.mkdir()
        run([compiler.resolve(), "build", HERE / "workload.hy", "--package", "capbench", "--out-dir", directory],
            HERE, out / f"{label}-emit.log", env)
        shutil.copyfile(HERE / "slice_test.go", directory / "slice_test.go")
        (directory / "go.mod").write_text("module capbench\n\ngo 1.26.9\n")
        run(["go", "test", "-c", "-o", directory / "bench"], directory, out / f"{label}-build.log", env)
    values = {label: {name: [] for name in EXPECTED} for label in ["main", "patched"]}
    command = ["-test.run=^$", "-test.bench=^BenchmarkSlice$", "-test.cpu=1",
               "-test.benchtime=500ms", "-test.count=1", "-test.benchmem"]
    for iteration in range(-1, SAMPLES):
        order = ["main", "patched"] if iteration % 2 == 0 else ["patched", "main"]
        for label in order:
            name = "warmup" if iteration == -1 else f"sample-{iteration + 1:02d}"
            text = run([out / label / "bench", *command], out / label, out / f"{name}-{label}.log", env)
            rows = LINE.findall(text)
            if {row[0] for row in rows} != EXPECTED or len(rows) != len(EXPECTED):
                raise RuntimeError(f"missing/duplicate benchmark rows in {name}-{label}.log")
            if iteration >= 0:
                for case, ns, size, alloc in rows:
                    values[label][case].append({"ns": float(ns), "bytes": int(size), "allocs": int(alloc)})
        print(f"{iteration + 1}/{SAMPLES} pairs complete", flush=True)
    summary = {}
    for name in sorted(EXPECTED):
        before = statistics.median(x["ns"] for x in values["main"][name])
        after = statistics.median(x["ns"] for x in values["patched"][name])
        ratio = after / before
        summary[name] = {"main_median_ns": before, "patched_median_ns": after,
                         "ratio": ratio, "pass": ratio <= 1.05,
                         "main": values["main"][name], "patched": values["patched"][name]}
    (out / "summary.json").write_text(json.dumps(summary, indent=2) + "\n")
    print(json.dumps({k: {a: b for a, b in v.items() if a not in ("main", "patched")}
                      for k, v in summary.items()}, indent=2))
    return 0 if all(case["pass"] for case in summary.values()) else 1


if __name__ == "__main__":
    raise SystemExit(main())
