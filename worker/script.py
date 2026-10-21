#!/usr/bin/env python3
"""Go-Hybrid generic Kaggle worker kernel (SPEC.md §14.2).

This file NEVER changes between jobs. tools/job.py injects the current job
spec into JOB_JSON below before `kaggle kernels push`. The kernel:
  1. prints an environment report,
  2. clones the PUBLIC repo at the pinned SHA (no secrets needed),
  3. runs the repo's own tools/runner/runner.py against the injected job.json,
  4. leaves results in /kaggle/working/results/ (fetched via kernels output).

No tokens, no secrets, no pushing — worker output is data.
"""
import json
import os
import shlex
import subprocess
import sys
import time

# --- injected by tools/job.py at submit time (single data line, runner logic untouched) ---
JOB_JSON = None  # INJECTED_BY_tools/job.py
# ---------------------------------------------------------------------------------------

WORK = "/kaggle/working" if os.path.isdir("/kaggle/working") else os.path.abspath("./work")


def sh(cmd, timeout=600, cwd=None):
    print("$ " + cmd, flush=True)
    try:
        p = subprocess.run(["/bin/sh", "-c", cmd], cwd=cwd, timeout=timeout,
                           stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
        out = p.stdout.decode("utf-8", "replace")
        print(out[-3000:], flush=True)
        return p.returncode, out
    except subprocess.TimeoutExpired:
        print("<<TIMEOUT {}s>>".format(timeout), flush=True)
        return 124, ""
    except Exception as e:  # noqa: BLE001
        print("<<error: {}>>".format(e), flush=True)
        return 125, ""


def main():
    t0 = time.time()
    os.makedirs(WORK, exist_ok=True)
    os.chdir(WORK)

    print("=== Go-Hybrid worker kernel ===", flush=True)
    print("WORK dir: {}  start: {}".format(WORK, time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())), flush=True)

    # 1. quick env report (full report comes from runner.py)
    for c in ("nproc", "free -m | head -2", "df -h /kaggle 2>/dev/null | head -2",
              "go version 2>/dev/null || echo 'no go'", "rustc --version 2>/dev/null || echo 'no rust'"):
        sh(c, 30)

    if not JOB_JSON:
        print("FATAL: no JOB_JSON injected; this kernel is only meant to be pushed by tools/job.py", flush=True)
        sys.exit(2)

    job = json.loads(JOB_JSON)
    job_path = os.path.join(WORK, "job.json")
    with open(job_path, "w") as f:
        json.dump(job, f, indent=2)

    # 2. clone the public repo at the pinned SHA
    repo = job.get("repo", "https://github.com/limitlessprojects/Go-Hybrid.git")
    sha = job.get("sha", "HEAD")
    rc, _ = sh("git clone --quiet {} repo 2>&1".format(shlex.quote(repo)), 300)
    if rc == 0:
        rc, _ = sh("cd repo && git checkout --quiet {} 2>&1".format(shlex.quote(sha)), 60)
    if rc != 0:
        # fall back to codeload tarball (sometimes git is restricted but https works)
        rc, _ = sh("rm -rf repo && mkdir repo && curl -fsSL --max-time 240 https://codeload.github.com/limitlessprojects/Go-Hybrid/tar.gz/{} | tar xz --strip-components=1 -C repo".format(shlex.quote(sha)), 300)
        if rc != 0:
            sys.exit(rc)

    # 3. run the repo's runner
    rc, _ = sh("python3 repo/tools/runner/runner.py {} --results-dir {}/results --repo-dir repo".format(job_path, WORK), 7200)

    # 4. surface summary
    try:
        with open(os.path.join(WORK, "results", "summary.json")) as f:
            s = json.load(f)
        print("\n=== JOB SUMMARY (job={}, ok={}, wall={:.0f}s) ===".format(s.get("job"), s.get("ok"), s.get("wall_clock_s") or 0), flush=True)
        for st in s.get("steps", []):
            print("  {} {} exit={} {:.1f}s".format(st["kind"], st["name"], st["exit"], st["dur_s"]), flush=True)
    except Exception as e:  # noqa: BLE001
        print("no summary.json: {}".format(e), flush=True)

    print("\nTotal kernel time: {:.0f}s".format(time.time() - t0), flush=True)
    # exit code of the kernel reflects the job (0 = all steps green)
    sys.exit(rc)


if __name__ == "__main__":
    main()
