#!/usr/bin/env python3
"""Queue sweeper — keeps the Go-Hybrid PR queue self-draining.

Policy (doctrine change 2026-10-24, see AUTOMATION.md):
  The old rule "the bot never closes" produced a 400+ PR review-debt pile
  while ~65 new PRs/day kept arriving. Now an open PR survives only if it is
  PROTECTED:
    - a protected label: triage:wildcard-hold / triage:pareto / triage:keep
    - its number is listed in tools/triage_config.json (wildcard_holds +
      drafts_parked) — survives even if labels are lost
    - it is younger than SWEEPER_MIN_AGE_DAYS (default 5)
  Everything older + unprotected is closed as not_planned WITH a written
  reply (the CONTRIBUTING "no silent closes" promise holds) that explains
  the reopen path. Closes never merge; landing stays human/autoland.

Runs daily via .github/workflows/queue-sweeper.yml. Safe to run locally.

Usage:
  python3 tools/queue_sweeper.py [--dry] [--max N] [--min-age D]
Env:
  GITHUB_TOKEN (Actions provides it), GITHUB_REPOSITORY (Actions provides it),
  SWEEPER_MIN_AGE_DAYS (default 5)
"""
import argparse, json, os, sys, time, urllib.request, urllib.error
from datetime import datetime, timedelta, timezone

REPO = os.environ.get("GITHUB_REPOSITORY", "limitlessprojects/Go-Hybrid")
TOK = os.environ.get("GITHUB_TOKEN", "")
MIN_AGE = int(os.environ.get("SWEEPER_MIN_AGE_DAYS", "5"))
PROTECTED_LABELS = {"triage:wildcard-hold", "triage:pareto", "triage:keep"}

COMMENT = """Queue sweeper: this PR has been open past the review window ({age} days)
without a protected label, so it is being closed as **not planned** to keep
the queue honest — the repo receives a high volume of PRs daily, and a stale
queue wastes every contributor's time, including yours.

This is **not a verdict on the work**. If the change still matters:

1. rebase onto current `main` (main lands changes continuously),
2. confirm the diff still applies and the affected verdicts still hold,
3. reopen — reopened PRs re-enter triage as fresh arrivals, and the daily
   triage board promotes land-candidates to `triage:pareto` (protected)
   automatically.

Wildcard holds (verdict window) and Pareto land candidates are never touched.
Every close gets this reply; nothing is ever closed silently."""

def api(path, method="GET", body=None):
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(f"https://api.github.com{path}", method=method, data=data,
        headers={"Authorization": f"token {TOK}", "User-Agent": "queue-sweeper",
                 "Accept": "application/vnd.github+json",
                 **({"Content-Type": "application/json"} if data else {})})
    for i in range(4):
        try:
            with urllib.request.urlopen(req, timeout=30) as r:
                raw = r.read()
                return r.status, (json.loads(raw) if raw else None)
        except urllib.error.HTTPError as e:
            if e.code in (403, 429):
                reset = e.headers.get("X-RateLimit-Reset")
                wait = max(10, int(reset) - int(time.time()) + 2) if reset else 180
                print(f"  rate-limited on {path}, sleeping {wait}s", flush=True)
                time.sleep(min(wait, 900)); continue
            return e.code, None
        except Exception as ex:
            print(f"  ERR {path}: {ex}", flush=True)
            time.sleep(4)
    return 0, None

def config_protected_numbers():
    nums = set()
    try:
        with open("tools/triage_config.json") as f:
            cfg = json.load(f)
        nums |= set(cfg.get("wildcard_holds", []))
        nums |= set(cfg.get("drafts_parked", []))
    except Exception as e:
        print(f"  (triage_config.json unreadable: {e} — label protection only)")
    return nums

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry", action="store_true")
    ap.add_argument("--max", type=int, default=200)
    ap.add_argument("--min-age", type=int, default=MIN_AGE)
    args = ap.parse_args()
    if not TOK:
        print("GITHUB_TOKEN not set"); sys.exit(1)

    cutoff = (datetime.now(timezone.utc) - timedelta(days=args.min_age)).isoformat()
    protected_nums = config_protected_numbers()
    print(f"repo={REPO} min_age={args.min_age}d cutoff={cutoff[:10]} "
          f"config_protected={len(protected_nums)} dry={args.dry}")

    prs, page = [], 1
    while page <= 10:
        st, d = api(f"/repos/{REPO}/pulls?state=open&per_page=100&page={page}")
        if st != 200 or not d: break
        prs += d
        if len(d) < 100: break
        page += 1
    print(f"open PRs: {len(prs)}")

    due = []
    for p in prs:
        labels = {l["name"] for l in p.get("labels", [])}
        if labels & PROTECTED_LABELS:
            continue
        if p["number"] in protected_nums:
            continue
        if (p.get("created_at") or "") >= cutoff:
            continue  # fresh arrival — still inside its review window
        due.append(p)
    due = due[: args.max]
    print(f"due for close: {len(due)}")
    if args.dry:
        for p in due[:25]:
            print(f"  would close #{p['number']:5d} [{p['user']['login']}] {p['title'][:60]}")
        print("DRY RUN — nothing closed")
        return

    n = fails = 0
    t0 = time.time()
    for p in due:
        age = (datetime.now(timezone.utc) -
               datetime.fromisoformat(p["created_at"].replace("Z", "+00:00"))).days
        st, _ = api(f"/repos/{REPO}/pulls/{p['number']}", "PATCH",
                    {"state": "closed", "state_reason": "not_planned"})
        ok = st in (200, 422)
        if ok:
            cs, _ = api(f"/repos/{REPO}/issues/{p['number']}/comments", "POST",
                        {"body": COMMENT.format(age=age)})
            ok = cs in (201, 403, 422)
        if ok: n += 1
        else: fails += 1; print(f"  FAIL #{p['number']} st={st}", flush=True)
        if (n + fails) % 25 == 0:
            print(f"  progress {n+fails}/{len(due)} closed={n} fails={fails} ({time.time()-t0:.0f}s)", flush=True)
        time.sleep(0.3)
    print(f"DONE: closed={n} fails={fails} elapsed={time.time()-t0:.0f}s")
    st, repo = api(f"/repos/{REPO}")
    if st == 200:
        print(f"open issues+PRs now: {repo.get('open_issues_count')}")

if __name__ == "__main__":
    main()
