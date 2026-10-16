#!/usr/bin/env python3
"""tools/job.py — tier-agnostic job control (SPEC.md §14.1).

  job.py kaggle submit  <jobdir>            resolve pinned SHA, inject, push kernel
  job.py kaggle status  [slug]              poll kernel status (one shot)
  job.py kaggle wait    <minutes> [slug]    poll until a NEW run terminates (0 ok/1 err/2 timeout)
  job.py kaggle fetch   <jobdir>             download kernel output -> jobdir/result/raw/
  job.py actions dispatch <workflow> <sha> <jobname> <shards> <maxparallel>
  job.py actions watch   [run_id]             poll latest heavy run (one shot)
  job.py actions fetch   <run_id> <jobdir>    download artifacts -> jobdir/result/ runs

Environment: KAGGLE_CLI (default /home/z/.venv/bin/kaggle), GITHUB_TOKEN read from
the git remote URL of this repo (never printed, never written to files).
Kaggle kernel slug: from worker/kernel-metadata.json (id field).
"""
import json
import math
import os
import re
import subprocess
import sys
import time
import urllib.request

REPO_DIR = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
WORKER_DIR = os.path.join(REPO_DIR, "worker")
META = os.path.join(WORKER_DIR, "kernel-metadata.json")
INJECT_MARKER = "JOB_JSON = None  # INJECTED_BY_tools/job.py"
GITHUB_API = "https://api.github.com"
GITHUB_REPO_API = GITHUB_API + "/repos/limitlessprojects/Go-Hybrid"
KAGGLE_CLI = os.environ.get("KAGGLE_CLI", "/home/z/.venv/bin/kaggle")


def sh(cmd, cwd=None, timeout=120):
    p = subprocess.run(["/bin/sh", "-c", cmd], cwd=cwd, timeout=timeout,
                       stdout=subprocess.PIPE, stderr=subprocess.STDOUT)
    return p.returncode, p.stdout.decode("utf-8", "replace").strip()


def gh_token():
    """Read the token from the environment or git remote (never print it)."""
    env_tok = os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN")
    if env_tok:
        return env_tok
    _, out = sh("git remote get-url origin", cwd=REPO_DIR)
    m = re.search(r"https://([^@]+)@github.com", out)
    return m.group(1) if m else None


def gh_api(path, data=None, method=None):
    tok = gh_token()
    req = urllib.request.Request(GITHUB_API + path)
    if tok:
        req.add_header("Authorization", "token " + tok)
    req.add_header("Accept", "application/vnd.github+json")
    if data is not None:
        req.add_header("Content-Type", "application/json")
        req.data = json.dumps(data).encode()
    if method:
        req.get_method = lambda: method
    with urllib.request.urlopen(req, timeout=60) as r:
        body = r.read().decode()
        return json.loads(body) if body.strip() else {"status": r.status}


def resolve_head():
    _, out = sh("git rev-parse HEAD", cwd=REPO_DIR)
    return out.strip()


def kaggle_slug():
    with open(META) as f:
        return json.load(f)["id"]


# ---------------------------------------------------------------- kaggle tier
def kaggle_submit(jobdir):
    head = resolve_head()
    with open(os.path.join(jobdir, "job.json")) as f:
        job = json.load(f)
    job["sha"] = head  # run the code we just pushed
    payload = json.dumps(job, indent=2)

    # sanity: no secrets in the payload
    for pat in ("ghp_", "gho_", "github_pat_", "KGAT_", "KAGGLE_KEY"):
        if pat in payload:
            sys.exit("REFUSING to push: secret pattern {} in job.json".format(pat))

    script = os.path.join(WORKER_DIR, "script.py")
    with open(script) as f:
        src = f.read()
    if INJECT_MARKER not in src:
        sys.exit("worker/script.py missing inject marker")
    injected = src.replace(INJECT_MARKER, "JOB_JSON = " + repr(payload))
    with open(script, "w") as f:
        f.write(injected)
    with open(os.path.join(WORKER_DIR, "job.json"), "w") as f:  # per SPEC §2 protocol
        f.write(payload)
    try:
        rc, out = sh("{} kernels push -p {}".format(KAGGLE_CLI, WORKER_DIR), timeout=180)
        print(out)
        if rc != 0:
            sys.exit("kaggle kernels push failed")
        # record what we submitted, with the resolved SHA
        with open(os.path.join(jobdir, "submitted.json"), "w") as f:
            json.dump({"sha": head, "submitted_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                       "job": job["job"]}, f, indent=2)
        print("submitted job {} at sha {}".format(job["job"], head[:10]))
    finally:
        # always restore the pristine template
        with open(script, "w") as f:
            f.write(src)
        # double safety: restore from git if anything went wrong
        sh("git checkout -- worker/script.py", cwd=REPO_DIR)
        if os.path.exists(os.path.join(WORKER_DIR, "job.json")):
            os.remove(os.path.join(WORKER_DIR, "job.json"))


def kaggle_status(slug=None):
    slug = slug or kaggle_slug()
    rc, out = sh("{} kernels status {}".format(KAGGLE_CLI, slug), timeout=60)
    print(out)
    return out


TERMINAL_STATES = ("COMPLETE", "ERROR", "CANCELACKNOWLEDGED", "CANCELEDEXECUTION")
ACTIVE_STATES = ("QUEUED", "RUNNING", "CANCELREQUESTED")


def kaggle_wait(minutes, slug=None):
    """Poll kernel status until a NEW run reaches a terminal state.

    Robustness rule: a terminal state is only accepted AFTER we have observed
    an active state (QUEUED/RUNNING) since this call started — otherwise a
    stale COMPLETE from the previous kernel version could be mistaken for the
    run we just submitted (queue latency is ~11s, so the active window is
    comfortably observable with a 5s poll interval).

    Exit codes: 0 = COMPLETE, 1 = kernel error/cancel, 2 = timeout.
    """
    slug = slug or kaggle_slug()
    deadline = time.time() + minutes * 60
    saw_active = False
    state, polls = "?", 0
    while time.time() < deadline:
        rc, out = sh("{} kernels status {}".format(KAGGLE_CLI, slug), timeout=60)
        polls += 1
        try:
            state = out.split('"')[-2].split(".")[-1].upper()  # '"KernelWorkerStatus.RUNNING"'
        except IndexError:
            state = "?"  # CLI error text: keep polling, do not trust
        print("[poll {}] {}".format(polls, state), flush=True)
        if state in ACTIVE_STATES:
            saw_active = True
        elif state in TERMINAL_STATES and saw_active:
            print("terminal after {} polls: {}".format(polls, state))
            return 0 if state == "COMPLETE" else 1
        time.sleep(5)
    print("TIMEOUT after {} min (last state {}, saw_active={})".format(minutes, state, saw_active))
    return 2


def kaggle_fetch(jobdir, slug=None):
    slug = slug or kaggle_slug()
    raw = os.path.join(jobdir, "result", "raw")
    os.makedirs(raw, exist_ok=True)
    rc, out = sh("{} kernels output {} -p {}".format(KAGGLE_CLI, slug, raw), timeout=180)
    print(out)
    # find summary.json anywhere under raw
    found = []
    for root, _, files in os.walk(raw):
        for fn in files:
            if fn == "summary.json":
                found.append(os.path.join(root, fn))
    if found:
        import shutil
        dst = os.path.join(jobdir, "result", "summary.json")
        shutil.copyfile(sorted(found)[-1], dst)
        print("summary.json -> " + dst)
        with open(dst) as f:
            print(json.dumps(json.load(f), indent=2)[:3000])
    else:
        print("WARNING: no summary.json in output")


# --------------------------------------------------------------- actions tier
def actions_dispatch(workflow, sha, jobname, shards, maxparallel):
    data = {"ref": "main", "inputs": {"sha": sha, "job": jobname,
                                       "shards": str(shards), "max-parallel": str(maxparallel)}}
    gh_api("/repos/limitlessprojects/Go-Hybrid/actions/workflows/{}.yml/dispatches".format(workflow), data, method="POST")
    print("dispatched {} sha={} job={} shards={} max-parallel={}".format(workflow, sha[:10], jobname, shards, maxparallel))


def actions_watch(run_id=None):
    if run_id is None:
        runs = gh_api("/repos/limitlessprojects/Go-Hybrid/actions/runs?per_page=5")["workflow_runs"]
        for r in runs:
            print("{}  {}  {}  sha={}".format(r["id"], r["name"], r["status"], r["head_sha"][:10]))
        return
    r = gh_api("/repos/limitlessprojects/Go-Hybrid/actions/runs/{}".format(run_id))
    print("run {} status={} conclusion={}".format(run_id, r["status"], r.get("conclusion")))


def actions_fetch(run_id, jobdir):
    tok = gh_token()
    if not tok:
        print("actions fetch requires GITHUB_TOKEN (set in environment or origin remote)", file=sys.stderr)
        sys.exit(1)
    arts = gh_api("/repos/limitlessprojects/Go-Hybrid/actions/runs/{}/artifacts".format(run_id))["artifacts"]
    os.makedirs(os.path.join(jobdir, "result"), exist_ok=True)
    for a in arts:
        url = a["archive_download_url"]
        dest = os.path.join(jobdir, "result", a["name"] + ".zip")
        req = urllib.request.Request(url)
        req.add_header("Authorization", "token " + tok)
        with urllib.request.urlopen(req, timeout=120) as r, open(dest, "wb") as f:
            f.write(r.read())
        print("artifact {} ({} bytes) -> {}".format(a["name"], a["size_in_bytes"], dest))


def main():
    if len(sys.argv) < 3:
        print(__doc__)
        sys.exit(2)
    tier, action = sys.argv[1], sys.argv[2]
    if tier == "kaggle" and action == "submit" and len(sys.argv) == 4:
        jobdir = os.path.abspath(sys.argv[3])
        if not os.path.isfile(os.path.join(jobdir, "job.json")):
            print("kaggle submit requires a job directory containing job.json", file=sys.stderr)
            sys.exit(2)
        kaggle_submit(jobdir)
    elif tier == "kaggle" and action == "status" and len(sys.argv) in (3, 4):
        kaggle_status(sys.argv[3] if len(sys.argv) == 4 else None)
    elif tier == "kaggle" and action == "wait" and len(sys.argv) in (4, 5):
        try:
            minutes = float(sys.argv[3])
            if not (math.isfinite(minutes) and minutes > 0):
                raise ValueError
        except ValueError:
            print("kaggle wait requires a positive number of minutes", file=sys.stderr)
            sys.exit(2)
        sys.exit(kaggle_wait(minutes, sys.argv[4] if len(sys.argv) == 5 else None))
    elif tier == "kaggle" and action == "fetch" and len(sys.argv) == 4:
        kaggle_fetch(os.path.abspath(sys.argv[3]))
    elif tier == "actions" and action == "dispatch" and len(sys.argv) == 8:
        try:
            shards, maxparallel = int(sys.argv[6]), int(sys.argv[7])
        except ValueError:
            print("actions dispatch requires positive integers for shards and maxparallel", file=sys.stderr)
            sys.exit(2)
        if shards <= 0 or maxparallel <= 0:
            print("actions dispatch requires positive integers for shards and maxparallel", file=sys.stderr)
            sys.exit(2)
        actions_dispatch(sys.argv[3], sys.argv[4], sys.argv[5], shards, maxparallel)
    elif tier == "actions" and action == "watch" and len(sys.argv) in (3, 4):
        run_id = None
        if len(sys.argv) == 4:
            try:
                run_id = int(sys.argv[3])
                if run_id <= 0:
                    raise ValueError
            except ValueError:
                print("actions watch requires a positive integer run_id", file=sys.stderr)
                sys.exit(2)
        actions_watch(run_id)
    elif tier == "actions" and action == "fetch" and len(sys.argv) == 5:
        try:
            run_id = int(sys.argv[3])
            if run_id <= 0:
                raise ValueError
        except ValueError:
            print("actions fetch requires a positive integer run_id", file=sys.stderr)
            sys.exit(2)
        actions_fetch(run_id, os.path.abspath(sys.argv[4]))
    else:
        print(__doc__)
        sys.exit(2)


if __name__ == "__main__":
    main()
