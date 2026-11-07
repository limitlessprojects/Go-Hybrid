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
    except subprocess.TimeoutExpired as exc:
        # communicate() has already collected output before the deadline. Keep it
        # so a slow build/test does not lose its last useful diagnostic.
        partial = exc.output or b""
        if isinstance(partial, bytes):
            partial = partial.decode("utf-8", "replace")
        code = 124
        out = partial + ("" if not partial or partial.endswith("\n") else "\n")
        out += "<<TIMEOUT after {}s>>".format(timeout)
    except Exception as e:  # noqa: BLE001 — runner must never die
        code, out = 125, "<<runner error: {}>>".format(e)
    out_tail = out[-4000:]
    return code, round(time.monotonic() - start, 2), out_tail


def stat_snapshot(path):
    """(mtime_ns, size) of an existing regular file, else None.

    Used to tell an artifact this run wrote from one that was already sitting
    in the results directory when the run started (issue #2191).
    """
    if not os.path.isfile(path):
        return None
    try:
        st = os.stat(path)
    except OSError:
        return None
    return (st.st_mtime_ns, st.st_size)


def resolve_expected(results_dir, expected):
    """Map a job's `results/...` spelling onto the worker's output root."""
    if expected.startswith("results/"):
        return os.path.join(results_dir, expected[len("results/"):])
    return expected



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
        # Go uses `version`, not `--version`. Avoid a pipe to head, whose
        # successful exit would hide a missing or failing tool's exit status.
        code, _, out = run_cmd([tool, "version" if tool == "go" else "--version"], 10)
        version = out.splitlines()[0].strip() if out.splitlines() else ""
        if code != 0 or not version or (tool == "go" and not version.startswith("go version ")):
            version = "n/a"
        rep["tool_" + tool] = version
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

    results_dir = args.results_dir
    os.makedirs(results_dir, exist_ok=True)
    repo_dir = args.repo_dir or os.environ.get("REPO_DIR", "repo")
    repo_dir = os.path.abspath(repo_dir)

    summary = {
        "schema": "go-hybrid-job-summary/1",
        "job": None,
        "sha": None,
        "tier": os.environ.get("JOB_TIER", "unknown"),
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

    def write_summary_and_exit():
        summary["wall_clock_s"] = round(time.monotonic() - t0, 2)
        summary["ok"] = bool(summary["steps"]) and all(s["exit"] == 0 for s in summary["steps"])
        summary["finished"] = UTC()
        path = os.path.join(results_dir, "summary.json")
        with open(path, "w") as f:
            json.dump(summary, f, indent=2)
        print("\n=== summary.json written to {} (ok={}) ===".format(path, summary["ok"]), flush=True)
        # exit non-zero if any step failed, so CI surfaces it
        sys.exit(0 if summary["ok"] else 1)

    try:
        with open(args.job_json, "r", encoding="utf-8") as f:
            job = json.load(f)
    except Exception as e:
        record("job-load", "check", 1, 0.0, "failed to read job file '{}': {}".format(args.job_json, e))
        write_summary_and_exit()

    if not isinstance(job, dict):
        record("job-spec", "check", 1, 0.0, "job spec must be a JSON object, got {}".format(type(job).__name__))
        write_summary_and_exit()

    summary["job"] = job.get("job")
    summary["sha"] = job.get("sha")
    summary["tier"] = job.get("tier", os.environ.get("JOB_TIER", "unknown"))

    setup_raw = job.get("setup", [])
    commands_raw = job.get("commands", [])

    if not isinstance(setup_raw, list):
        record("job-spec", "check", 1, 0.0, "'setup' must be a list, got {}".format(type(setup_raw).__name__))
        write_summary_and_exit()

    if not isinstance(commands_raw, list):
        record("job-spec", "check", 1, 0.0, "'commands' must be a list, got {}".format(type(commands_raw).__name__))
        write_summary_and_exit()

    if not setup_raw and not commands_raw:
        record("job-spec", "check", 1, 0.0, "job defines no executable setup or commands")
        write_summary_and_exit()

    # sanity: verify we are at the pinned SHA when a repo checkout exists
    sha_ok = True
    unverifiable_pin = False
    want = job.get("sha")
    # Linked worktrees and submodules use a .git file, not a directory.
    # Even a broken metadata link or corrupt .git directory inside an ancestor
    # git repository must enter the fail-closed SHA check without walking up to
    # an enclosing parent repository.
    if os.path.lexists(os.path.join(repo_dir, ".git")):
        sha_env = dict(os.environ, GIT_CEILING_DIRECTORIES=os.path.dirname(repo_dir))
        code, _, out = run_cmd(["git", "rev-parse", "HEAD"], 10, cwd=repo_dir, env=sha_env)
        at = out.strip()
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
    elif want not in (None, "HEAD", ""):
        # A pinned job against a checkout with no .git (e.g. the Kaggle worker's
        # codeload-tarball fallback): there is nothing to verify the pin against,
        # so the job must not run or report ok. Reporting green here would make
        # summary.json evidence for a commit that was never checked out.
        sha_ok = False
        unverifiable_pin = True
        record(
            "sha-check",
            "check",
            1,
            0.0,
            "no .git in {}; cannot verify pinned sha {}".format(repo_dir, str(want)[:12]),
        )

    hard_setup_failed = False
    # Snapshot every declared output that is already on disk, before any job
    # step runs. `os.path.isfile` on its own let a file left behind by an
    # earlier invocation certify a run that produced nothing (issue #2191).
    pre_run_results = {}
    declared_results = job.get("expected_results", [])
    if isinstance(declared_results, list):
        for expected in declared_results:
            if not isinstance(expected, str) or not expected:
                continue
            target = resolve_expected(results_dir, expected)
            if os.path.abspath(target) == os.path.abspath(os.path.join(results_dir, "summary.json")):
                continue
            snapshot = stat_snapshot(target)
            if snapshot is not None:
                pre_run_results[expected] = snapshot
    if not sha_ok:
        print(
            "[{}] check sha-check: failed ({}), skipping setup and commands".format(
                UTC(), "pin unverifiable" if unverifiable_pin else "commit mismatch"
            ),
            flush=True,
        )
    else:
        for idx, step in enumerate(setup_raw):
            if not isinstance(step, dict):
                record("setup-{}".format(idx), "setup", 1, 0.0, "step must be an object, got {}".format(type(step).__name__))
                hard_setup_failed = True
                break
            name = step.get("name") or "setup-{}".format(idx)
            cmd = step.get("run")
            if not cmd:
                record(name, "setup", 1, 0.0, "step missing required 'run' command")
                if step.get("hard"):
                    hard_setup_failed = True
                    break
                continue
            cwd = repo_dir if step.get("cwd") == "repo" else (step.get("cwd") or None)
            code, dur, out = run_cmd(cmd, step.get("timeout", 600), cwd=cwd)
            record(name, "setup", code, dur, out)
            if code != 0 and step.get("hard"):
                hard_setup_failed = True
                break

    commands = [] if (hard_setup_failed or not sha_ok) else commands_raw
    for idx, step in enumerate(commands):
        if not isinstance(step, dict):
            record("cmd-{}".format(idx), "cmd", 1, 0.0, "command step must be an object, got {}".format(type(step).__name__))
            break
        name = step.get("name") or "cmd-{}".format(idx)
        cmd = step.get("run")
        if not cmd:
            record(name, "cmd", 1, 0.0, "command step missing required 'run' command")
            if step.get("hard"):
                break
            continue
        cwd = repo_dir if step.get("cwd") == "repo" else (step.get("cwd") or None)
        code, dur, out = run_cmd(cmd, step.get("timeout", 600), cwd=cwd)
        record(name, "cmd", code, dur, out)
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
        target = resolve_expected(results_dir, expected)
        # This file is generated below, after the final verdict is known.
        if os.path.abspath(target) == os.path.abspath(path):
            continue
        if not os.path.isfile(target):
            record("expected-result", "check", 1, 0.0,
                   "missing expected result file: {}".format(expected))
        elif pre_run_results.get(expected) is not None and stat_snapshot(target) == pre_run_results[expected]:
            # Byte-for-byte the file that was already there when the run
            # started: a leftover artifact, not this run's evidence (#2191).
            record("expected-result", "check", 1, 0.0,
                   "expected result file was not produced by this run "
                   "(unchanged since before it started): {}".format(expected))

    write_summary_and_exit()


if __name__ == "__main__":
    main()
