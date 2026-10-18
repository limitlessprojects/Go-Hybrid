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

Exit codes: 0 = success, 1 = the fetch retrieved nothing (failed download,
no artifacts, or no summary.json), 2 = usage error.
"""
import json
import os
import re
import subprocess
import sys
import tempfile
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
    """Read the token from the git remote (never print it)."""
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
    rc, out = sh("git rev-parse --verify HEAD", cwd=REPO_DIR)
    if rc != 0:
        sys.exit("Cannot resolve HEAD; refusing to submit a job without a valid revision pin")
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
    with open(script, "rb") as f:
        original_script = f.read()
    src = original_script.decode("utf-8")
    if INJECT_MARKER not in src:
        sys.exit("worker/script.py missing inject marker")
    injected = src.replace(INJECT_MARKER, "JOB_JSON = " + repr(payload))
    sidecar = os.path.join(WORKER_DIR, "job.json")
    try:
        with open(sidecar, "rb") as f:
            original_sidecar = f.read()
    except FileNotFoundError:
        original_sidecar = None
    try:
        # Include preparation in the cleanup scope: a failed sidecar write
        # must not leave an injected template behind.
        with open(script, "wb") as f:
            f.write(injected.encode("utf-8"))
        with open(sidecar, "w", encoding="utf-8") as f:  # per SPEC §2 protocol
            f.write(payload)
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
        # Restore the caller's exact files, including uncommitted edits and
        # line endings. A git checkout here would silently discard local work.
        with open(script, "wb") as f:
            f.write(original_script)
        if original_sidecar is not None:
            with open(sidecar, "wb") as f:
                f.write(original_sidecar)
        elif os.path.exists(sidecar):
            os.remove(sidecar)


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
    # A failed download must not read as a successful fetch: an empty result
    # dir is indistinguishable from "the kernel produced nothing".
    if rc != 0:
        print("kaggle fetch: kernels output exited {} (download failed)".format(rc), file=sys.stderr)
        return 1
    # find summary.json anywhere under raw
    found = []
    for root, _, files in os.walk(raw):
        for fn in files:
            if fn == "summary.json":
                found.append(os.path.join(root, fn))
    if found:
        import shutil
        source = sorted(found)[-1]
        # A successful transfer may still contain an older kernel's result.
        # Validate before replacing the local summary used as job evidence.
        try:
            with open(source, encoding="utf-8") as f:
                summary = json.load(f)
            if not isinstance(summary, dict):
                raise ValueError("summary.json must be an object")
            submitted_path = os.path.join(jobdir, "submitted.json")
            try:
                with open(submitted_path, encoding="utf-8") as f:
                    submitted = json.load(f)
            except FileNotFoundError:
                submitted = None  # Support fetching jobs submitted elsewhere.
            else:
                if not isinstance(submitted, dict) or any(
                    not isinstance(submitted.get(key), str) or not submitted[key]
                    for key in ("job", "sha")
                ):
                    raise ValueError("submitted.json must identify the job and revision")
                for key in ("job", "sha"):
                    if summary.get(key) != submitted[key]:
                        raise ValueError("summary.json {} does not match submitted.json".format(key))
        except (OSError, ValueError) as error:
            sys.exit("REFUSING to promote fetched summary: {}".format(error))
        dst = os.path.join(jobdir, "result", "summary.json")
        shutil.copyfile(source, dst)
        print("summary.json -> " + dst)
        with open(dst) as f:
            print(json.dumps(json.load(f), indent=2)[:3000])
        return 0
    # No summary.json is a hard failure for a fetch: nothing was retrieved to
    # inspect. Exit non-zero so a caller (and CI) cannot treat it as success.
    print("kaggle fetch: no summary.json under {} (nothing retrieved)".format(raw), file=sys.stderr)
    return 1


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
    # This endpoint defaults to only 30 results. Fetch every page before
    # downloading so a sharded run cannot silently lose later artifacts.
    arts = []
    page = 1
    per_page = 100
    while True:
        payload = gh_api(
            "/repos/limitlessprojects/Go-Hybrid/actions/runs/{}/artifacts?per_page={}&page={}".format(
                run_id, per_page, page
            )
        )
        # An error payload (rate limit, missing run, bad token) carries no
        # "artifacts" key; fail loudly instead of reporting an empty fetch.
        if "artifacts" not in payload:
            print(
                "actions fetch: run {} artifact list unavailable: {}".format(
                    run_id, payload.get("message") or payload.get("status") or "no artifacts key"
                ),
                file=sys.stderr,
            )
            return 1
        batch = payload["artifacts"]
        if not batch:
            break
        arts.extend(batch)
        if len(batch) < per_page:
            break
        page += 1
    if not arts:
        print("actions fetch: run {} has no artifacts to download".format(run_id), file=sys.stderr)
        return 1
    os.makedirs(os.path.join(jobdir, "result"), exist_ok=True)
    tok = gh_token()
    for a in arts:
        url = a["archive_download_url"]
        dest = os.path.join(jobdir, "result", a["name"] + ".zip")
        req = urllib.request.Request(url)
        req.add_header("Authorization", "token " + tok)
        # Finish the response before touching a previous valid archive. Publish
        # via a sibling temporary file so write/replace failures preserve it too.
        with urllib.request.urlopen(req, timeout=120) as r:
            data = r.read()
        temporary = None
        try:
            with tempfile.NamedTemporaryFile(dir=os.path.dirname(dest),
                                             prefix=".artifact-", delete=False) as f:
                temporary = f.name
                f.write(data)
            os.replace(temporary, dest)
        finally:
            if temporary is not None and os.path.exists(temporary):
                os.remove(temporary)
        print("artifact {} ({} bytes) -> {}".format(a["name"], a["size_in_bytes"], dest))
    return 0


def main():
    if len(sys.argv) < 3:
        print(__doc__)
        sys.exit(2)
    tier, action = sys.argv[1], sys.argv[2]
    if tier == "kaggle" and action == "submit" and len(sys.argv) == 4:
        kaggle_submit(os.path.abspath(sys.argv[3]))
    elif tier == "kaggle" and action == "status":
        kaggle_status(sys.argv[4] if len(sys.argv) > 4 else None)
    elif tier == "kaggle" and action == "wait" and len(sys.argv) >= 4:
        minutes = float(sys.argv[3])
        sys.exit(kaggle_wait(minutes, sys.argv[4] if len(sys.argv) > 4 else None))
    elif tier == "kaggle" and action == "fetch" and len(sys.argv) == 4:
        sys.exit(kaggle_fetch(os.path.abspath(sys.argv[3])))
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
    elif tier == "actions" and action == "watch":
        actions_watch(int(sys.argv[4]) if len(sys.argv) > 4 else None)
    elif tier == "actions" and action == "fetch" and len(sys.argv) == 5:
        sys.exit(actions_fetch(int(sys.argv[3]), os.path.abspath(sys.argv[4])))
    else:
        print(__doc__)
        sys.exit(2)


if __name__ == "__main__":
    main()
