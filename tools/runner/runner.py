#!/usr/bin/env python3
"""Generic job runner — the single execution path for all three compute tiers
(sandbox, Kaggle worker, GitHub Actions shards). See SPEC.md §14.1.

Usage:  python3 tools/runner/runner.py <job.json> [--results-dir DIR]

The job.json schema (version 1):
{
  "job": "001-spike0-kaggle-smoke",        # unique job name
  "tier": "any",                            # informational
  "sha": "abcdef0...",                      # pinned commit this job must run
  "repo": "https://github.com/limitlessprojects/Go-Hybrid.git",
  "gpu": false,
  "env_report": true,                       # always print the environment report
  "setup": [                                # optional setup steps (not timed as results)
      {"name": "install-go", "run": "...", "timeout": 600}
  ],
  "commands": [                             # timed commands; each produces exit code+duration
      {"name": "hello-go", "run": "go build ./...", "timeout": 300, "cwd": "repo"}
  ],
  "expected_results": ["results/summary.json"]
}

Behavior:
  * Always writes summary.json (even on failure) with: job, sha, tier, hardware report,
    toolchain versions, per-command exit codes + durations, wall clock.
  * Steps run under timeout; failures do not stop later steps (fail-soft) unless
    "hard": true. A hard setup failure skips all commands and still writes a summary.
  * "cwd": "repo" means relative to the cloned repo checkout.
  * Output files land in the results dir (default: $RESULTS_DIR or ./results).
  * Declared expected_results must be files. A results/ prefix refers to the
    selected results dir, including when --results-dir overrides the default.
  * Never panics; all errors become entries in summary.json.
Stdlib only — must run on a bare Kaggle/Actions image with just python3.
"""
import argparse
import datetime
import json
import os
import platform
import shutil
import subprocess
import sys
import time

UTC = lambda: datetime.datetime.now(datetime.timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ")


def run_cmd(cmd, timeout, cwd=None, env=None):
    """Run one command; return (exit_code, duration_s, tail_of_output). Never raises."""
    start = time.monotonic()
    out_tail = ""
    try:
        p = subprocess.run(
            cmd if isinstance(cmd, list) else ["/bin/sh", "-c", cmd],
            cwd=cwd, env=env,
            stdout=subprocess.PIPE, stderr=subprocess.STDOUT,
            timeout=timeout,
        )
        code, out = p.returncode, p.stdout.decode("utf-8", "replace")
    except subprocess.TimeoutExpired:
        code, out = 124, "<<TIMEOUT after {}s>>".format(timeout)
    except Exception as e:  # noqa: BLE001 — runner must never die
        code, out = 125, "<<runner error: {}>>".format(e)
    out_tail = out[-4000:]
    return code, round(time.monotonic() - start, 2), out_tail


def env_report():
    """Hardware + toolchain report; the same on every tier."""
    rep = {
        "ts": UTC(),
        "platform": platform.platform(),
        "python": sys.version.split()[0],
        "machine": platform.machine(),
    }
    def cap(cmd):
        c, _, out = run_cmd(cmd, 10)
        return out.strip() if c == 0 else "n/a"
    rep["nproc"] = cap("nproc")
    rep["lscpu"] = cap("lscpu | head -20")
    rep["mem"] = cap("free -m")
    rep["disk"] = cap("df -h / /tmp 2>/dev/null | head -4")
    rep["cgroup_mem"] = cap("cat /sys/fs/cgroup/memory.max 2>/dev/null || cat /sys/fs/cgroup/memory/memory.limit_in_bytes 2>/dev/null")
    rep["cgroup_cpu"] = cap("cat /sys/fs/cgroup/cpu.max 2>/dev/null || cat /sys/fs/cgroup/cpu/cpu.cfs_quota_us 2>/dev/null")
    for tool in ("go", "rustc", "cargo", "gcc", "clang", "git", "cc"):
        rep["tool_" + tool] = cap("{} --version 2>/dev/null | head -1".format(tool))
    rep["kaggle_env"] = redact({k: v for k, v in os.environ.items() if k.startswith("KAGGLE_")})
    rep["ci_env"] = redact({k: v for k, v in os.environ.items() if k.startswith(("GITHUB_", "RUNNER_", "CI"))})
    return rep


# Env values that look secret-ish are masked in reports (R10 hygiene).
# Lesson 2026-10-12: Kaggle embeds ephemeral session JWTs (KAGGLE_DATA_PROXY_TOKEN,
# KAGGLE_USER_SECRETS_TOKEN) in kernel env; they leaked into committed summaries once.
_SECRET_KEY = ("TOKEN", "SECRET", "PASSWORD", "CREDENTIAL", "_KEY", "API_KEY", "PAT")


def redact(env: dict) -> dict:
    out = {}
    for k, v in env.items():
        if any(s in k.upper() for s in _SECRET_KEY):
            out[k] = "[REDACTED len={}]".format(len(v)) if v else "[EMPTY]"
        else:
            out[k] = v
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("job_json")
    ap.add_argument("--results-dir", default=os.environ.get("RESULTS_DIR", "results"))
    ap.add_argument("--repo-dir", default=None, help="where the repo was cloned (default: $REPO_DIR or ./repo)")
    args = ap.parse_args()

    with open(args.job_json) as f:
        job = json.load(f)

    results_dir = args.results_dir
    os.makedirs(results_dir, exist_ok=True)
    repo_dir = args.repo_dir or os.environ.get("REPO_DIR", "repo")
    repo_dir = os.path.abspath(repo_dir)

    summary = {
        "schema": "go-hybrid-job-summary/1",
        "job": job.get("job"),
        "sha": job.get("sha"),
        "tier": job.get("tier", os.environ.get("JOB_TIER", "unknown")),
        "started": UTC(),
        "hardware": env_report(),
        "steps": [],
        "wall_clock_s": None,
        "ok": False,
    }
    # export for job commands (portable across tiers)
    os.environ["RESULTS_DIR"] = os.path.abspath(results_dir)
    os.environ["REPO_DIR"] = repo_dir
    t0 = time.monotonic()

    def record(name, kind, code, dur, out):
        summary["steps"].append(
            {"name": name, "kind": kind, "exit": code, "dur_s": dur, "out_tail": out[-1500:]}
        )
        print("[{}] {} {}: exit={} {:.1f}s".format(UTC(), kind, name, code, dur), flush=True)

    # sanity: verify we are at the pinned SHA when a repo checkout exists
    sha_ok = True
    if os.path.isdir(os.path.join(repo_dir, ".git")):
        code, _, out = run_cmd(["git", "rev-parse", "HEAD"], 10, cwd=repo_dir)
        at = out.strip()
        want = job.get("sha")
        if code != 0:
            sha_ok = False
            record("sha-check", "check", 1, 0.0, "failed to resolve HEAD: {}".format(out.strip()))
        elif want in (None, "HEAD", ""):
            record("sha-check", "check", 0, 0.0, "job pins HEAD; at {}".format(at[:12]))
        elif at != want:
            # shallow clones of a SHA may differ if fetch fell back to a branch; log loudly
            sha_ok = False
            record("sha-check", "check", 1, 0.0, "repo at {} but job pins {}".format(at, want))
        else:
            record("sha-check", "check", 0, 0.0, "at {}".format(at))

    hard_setup_failed = False
    if not sha_ok:
        print("[{}] check sha-check: failed, skipping setup and commands".format(UTC()), flush=True)
    else:
        for step in job.get("setup", []):
            cwd = repo_dir if step.get("cwd") == "repo" else (step.get("cwd") or None)
            code, dur, out = run_cmd(step["run"], step.get("timeout", 600), cwd=cwd)
            record(step["name"], "setup", code, dur, out)
            if code != 0 and step.get("hard"):
                hard_setup_failed = True
                break

    commands = [] if (hard_setup_failed or not sha_ok) else job.get("commands", [])
    for step in commands:
        cwd = repo_dir if step.get("cwd") == "repo" else (step.get("cwd") or None)
        code, dur, out = run_cmd(step["run"], step.get("timeout", 600), cwd=cwd)
        record(step["name"], "cmd", code, dur, out)
        if code != 0 and step.get("hard"):
            break

    path = os.path.join(results_dir, "summary.json")
    expected_results = job.get("expected_results", [])
    if not isinstance(expected_results, list):
        record("expected-result", "check", 1, 0.0,
               "expected_results must be a list of file paths")
        expected_results = []
    for expected in expected_results:
        if not isinstance(expected, str) or not expected:
            record("expected-result", "check", 1, 0.0,
                   "expected result must be a non-empty file path")
            continue
        # Job specs use results/... independent of the worker's output root.
        target = (os.path.join(results_dir, expected[len("results/"):])
                  if expected.startswith("results/") else expected)
        # This file is generated below, after the final verdict is known.
        if os.path.abspath(target) == os.path.abspath(path):
            continue
        if not os.path.isfile(target):
            record("expected-result", "check", 1, 0.0,
                   "missing expected result file: {}".format(expected))

    summary["wall_clock_s"] = round(time.monotonic() - t0, 2)
    summary["ok"] = all(s["exit"] == 0 for s in summary["steps"])
    summary["finished"] = UTC()

    with open(path, "w") as f:
        json.dump(summary, f, indent=2)
    print("\n=== summary.json written to {} (ok={}) ===".format(path, summary["ok"]), flush=True)
    # exit non-zero if any step failed, so CI surfaces it
    sys.exit(0 if summary["ok"] else 1)


if __name__ == "__main__":
    main()
