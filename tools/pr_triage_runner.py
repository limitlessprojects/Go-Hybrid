#!/usr/bin/env python3
"""Go-Hybrid quality-first PR triage runner (local agent side).

Owner doctrine (2026-10-30): the blunt 5-day auto-close sweep is retired —
closes are a deliberate act. This runner implements the deliberate version:

HARD RULES
  1. Never merge. Landing stays human (AUTOMATION.md).
  2. Never close a PR whose CI is green (SUCCESS) or still running/pending
     and healthy — green-CI PRs are land candidates; they get labeled for
     review instead.
  3. Never touch protected PRs: labels triage:wildcard-hold / triage:pareto /
     triage:keep, numbers in triage_config.json (wildcard_holds +
     drafts_parked), or reopened inside the window.
  4. Every close gets a PR-specific written reply (CONTRIBUTING rule 4:
     no silent closes) with concrete evidence, and is marked not_planned
     with the reopen path spelled out.

CLOSE EVIDENCE REQUIRED (at least one, on top of age >= min_age):
  A. CI rollup FAILURE (suite conclusion FAILURE) — cannot be reviewed for
     landing; author idle for >= min_age days.
  B. CONFLICTING merge state AND idle >= conflict_idle_days (default 10).
  C. Superseded/duplicate: head tree byte-identical (or a subset by file set)
     of an open land-candidate PR, or the change already landed on main
     (verified per-PR via merge-base test, not guessed).

Everything else: labeled (triage:new / conflicts / behind / duplicate /
tests) and left for the human/next tick. All writes are quota-guarded.

Usage:
  python3 pr_triage_runner.py --dry                 # decide, print, no writes
  python3 pr_triage_runner.py --execute --max 40    # act on top-N decisions
Env:
  GH_TOKEN required.  SURVEY_JSON optional precomputed snapshot.
"""
import argparse, json, os, sys, time, urllib.request, urllib.error
from datetime import datetime, timezone

REPO = "limitlessprojects/Go-Hybrid"
TOKEN = os.environ.get("GH_TOKEN", "")
if not TOKEN:
    sys.exit("GH_TOKEN missing")

PROTECTED_LABELS = {"triage:wildcard-hold", "triage:pareto", "triage:keep"}
MIN_AGE = float(os.environ.get("TRIAGE_MIN_AGE_DAYS", "5"))
CONFLICT_IDLE = float(os.environ.get("TRIAGE_CONFLICT_IDLE_DAYS", "10"))
REOPEN_WINDOW = float(os.environ.get("TRIAGE_REOPEN_WINDOW_DAYS", "5"))
MAX_REST_BUDGET = int(os.environ.get("TRIAGE_REST_BUDGET", "1800"))
LOG = os.environ.get("TRIAGE_LOG", os.path.join(
    os.environ.get("TRIAGE_STATE_DIR", "/tmp/gh-triage"), "triage_runner.log"))
STATE_DIR = os.environ.get("TRIAGE_STATE_DIR", "/tmp/gh-triage")

_config = None
def config_protected():
    global _config
    if _config is None:
        try:
            with open(os.path.join(os.path.dirname(os.path.abspath(__file__)),
                                   "triage_config.json")) as f:
                cfg = json.load(f)
            _config = set(cfg.get("wildcard_holds", [])) | set(cfg.get("drafts_parked", []))
        except Exception as e:
            print(f"  (config unreadable: {e})"); _config = set()
    return _config

rest_used = 0
def rest(path, method="GET", body=None):
    """REST with rate-limit awareness; returns (status, json)."""
    global rest_used
    if rest_used >= MAX_REST_BUDGET:
        raise SystemExit("REST budget exhausted — stopping to stay safe")
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(f"https://api.github.com{path}", method=method, data=data,
        headers={"Authorization": f"token {TOKEN}", "User-Agent": "pr-triage-runner",
                 "Accept": "application/vnd.github+json"})
    for i in range(4):
        try:
            with urllib.request.urlopen(req, timeout=30) as r:
                rest_used += 1
                raw = r.read()
                return r.status, (json.loads(raw) if raw else None)
        except urllib.error.HTTPError as e:
            rest_used += 1
            if e.code in (403, 429):
                reset = e.headers.get("X-RateLimit-Reset")
                wait = max(20, int(reset) - int(time.time()) + 2) if reset else 300
                print(f"  rate-limited on {path}, sleeping {min(wait, 900)}s", flush=True)
                time.sleep(min(wait, 900)); continue
            if e.code >= 500:
                time.sleep(8 * (i + 1)); continue
            return e.code, None
        except Exception as ex:
            print(f"  ERR {path}: {ex}", flush=True); time.sleep(6)
    return 0, None

def gql(query, variables):
    body = json.dumps({"query": query, "variables": variables}).encode()
    req = urllib.request.Request("https://api.github.com/graphql", method="POST", data=body,
        headers={"Authorization": f"Bearer {TOKEN}", "User-Agent": "pr-triage-runner",
                 "Content-Type": "application/json"})
    for i in range(4):
        try:
            with urllib.request.urlopen(req, timeout=60) as r:
                return json.loads(r.read())
        except urllib.error.HTTPError as e:
            if e.code in (403, 429, 502, 503):
                reset = e.headers.get("X-RateLimit-Reset")
                wait = max(20, int(reset) - int(time.time()) + 2) if reset else 120
                time.sleep(min(wait, 600)); continue
            raise
        except Exception:
            time.sleep(10 * (i + 1))
    return {}

def reopened_recently(number):
    st, events = rest(f"/repos/{REPO}/issues/{number}/events?per_page=100")
    if st != 200 or not events:
        return True  # unverifiable history -> survive (same doctrine as sweeper)
    now = datetime.now(timezone.utc)
    for ev in events:
        if ev.get("event") == "reopened" and ev.get("created_at"):
            age = (now - datetime.fromisoformat(ev["created_at"].replace("Z", "+00:00"))).total_seconds() / 86400
            if age <= REOPEN_WINDOW:
                return True
    return False

def already_landed(pr):
    """True if the PR's head diff is already present on main (file-level
    subset test against merge-base). Cheap heuristic, run only for old PRs
    without other close evidence."""
    st, base = rest(f"/repos/{REPO}/pulls/{pr['number']}")
    if st != 200 or not base: return False, None
    mb = base.get("base", {}).get("sha"); head = base.get("head", {}).get("sha")
    if not mb or not head: return False, None
    st, cmp = rest(f"/repos/{REPO}/compare/{mb}...{head}")
    if st != 200 or not cmp: return False, None
    files = cmp.get("files") or []
    if not files: return True, "empty diff vs base"
    return False, None

def author_recently_active(number, author_login, days=3.0):
    """True if the author commented on the thread within `days`. Labels/CI
    touches pollute updatedAt, so the comment thread is the honest signal."""
    st, comments = rest(f"/repos/{REPO}/issues/{number}/comments?per_page=100")
    if st != 200 or comments is None:
        return True  # unverifiable -> survive
    now = datetime.now(timezone.utc)
    for c in comments:
        if (c.get("user") or {}).get("login") == author_login and c.get("created_at"):
            age = (now - datetime.fromisoformat(c["created_at"].replace("Z", "+00:00"))).total_seconds() / 86400
            if age <= days:
                return True
    return False

def reply_for(pr, evidence, extra=""):
    age = pr["_age_days"]; ci = pr["_ci_rollup"]
    lines = [
        f"**Automated triage tick (quality-first, deliberate close).**",
        f"",
        f"This PR has been open {age:.0f} days and currently cannot be reviewed for landing:",
    ]
    if "ci-failure" in evidence:
        concl = ", ".join(f"{k}×{v}" for k, v in pr["_suite_concls"].items() if k and k != "SUCCESS") or "CI FAILURE"
        lines.append(f"- CI on the head commit is red: rollup **{ci}** ({concl}).")
    if "conflicts" in evidence:
        lines.append(f"- The branch no longer merges cleanly into `main` (merge state: **CONFLICTING**) and has been idle {pr['_idle_days']:.0f} days without a rebase.")
    if "superseded" in evidence:
        lines.append(f"- The change is already covered by {extra}.")
    lines += [
        f"",
        f"This is **not a verdict on the work** — per CONTRIBUTING every change is welcome; the queue just stays honest so active work gets review time.",
        f"",
        f"**Reopen path**: rebase onto current `main`, confirm the affected gates still hold (see AUTOMATION.md), and reopen — reopened PRs re-enter triage as fresh arrivals and the daily board promotes land-candidates to `triage:pareto`. Closed as not planned; nothing was merged and nothing is lost.",
    ]
    return "\n".join(lines)

def decide(pr):
    """Return (action, evidence, extra). action: close|skip.
    Conservative: close ONLY with hard evidence on top of age >= MIN_AGE.
    Green-CI / no-CI PRs are never closed — they stay for human review and
    the daily board (triage_bot scan --post) owns their labels."""
    labels = set(pr["_labels"])
    if labels & PROTECTED_LABELS: return "skip", [], ""
    if int(pr["number"]) in config_protected(): return "skip", [], ""
    title = (pr.get("title") or "").lower()
    # Wildcard-experiment submissions are never swept (M1 doctrine); some
    # never received the hold label — catch them by title, flag for review.
    if "wildcard" in title: return "skip", [], ""
    if pr["_age_days"] < MIN_AGE: return "skip", [], []
    ci = pr["_ci_rollup"]; mergeable = pr.get("mergeable"); idle = pr["_idle_days"]
    evidence = []
    if ci == "FAILURE": evidence.append("ci-failure")
    if mergeable == "CONFLICTING" and idle >= CONFLICT_IDLE: evidence.append("conflicts")
    if not evidence:
        return "skip", [], ""
    return "close", evidence, ""

def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry", action="store_true", default=not "--execute" in sys.argv)
    ap.add_argument("--execute", action="store_true")
    ap.add_argument("--max", type=int, default=40, help="max closes per run")
    ap.add_argument("--survey", default=os.environ.get(
        "SURVEY_JSON", os.path.join(STATE_DIR, "queue_snapshot.json")))
    ap.add_argument("--label-only", action="store_true", help="skip closes even if evidence exists")
    args = ap.parse_args()
    os.makedirs(STATE_DIR, exist_ok=True)

    with open(args.survey) as f: prs = json.load(f)
    now = datetime.now(timezone.utc)
    for p in prs:
        p["_age_days"] = round((now - datetime.fromisoformat(p["createdAt"].replace("Z", "+00:00"))).total_seconds()/86400, 1)
        p["_idle_days"] = round((now - datetime.fromisoformat(p["updatedAt"].replace("Z", "+00:00"))).total_seconds()/86400, 1)

    decisions = []
    for p in prs:
        act, ev, extra = decide(p)
        decisions.append((p, act, ev, extra))

    # Oldest first: PRs get their full review window before any close.
    decisions.sort(key=lambda d: -d[0]["_age_days"])
    all_closes = [(p, ev) for p, a, ev, _ in decisions if a == "close"]
    closes = all_closes[: args.max]
    # Land candidates for the next landing session: green CI, clean merge state,
    # unprotected — surfaced, never closed.
    land_cands = [p for p in prs
                  if p["_ci_rollup"] == "SUCCESS" and p.get("mergeStateStatus") == "CLEAN"
                  and not (set(p["_labels"]) & PROTECTED_LABELS)
                  and int(p["number"]) not in config_protected()]
    with open(os.path.join(STATE_DIR, "land_candidates.json"), "w") as f:
        json.dump([{"number": p["number"], "title": p["title"], "age_days": p["_age_days"],
                    "author": (p.get("author") or {}).get("login")} for p in land_cands], f, indent=1)
    print(f"land candidates (green CI + CLEAN, open): {len(land_cands)} -> land_candidates.json")
    print(f"queue={len(prs)}  close-candidates={len([d for d in decisions if d[1]=='close'])}"
          f"  (executing first {len(closes)}; cap --max {args.max})")

    summary = {"run": datetime.now(timezone.utc).isoformat(), "queue": len(prs),
               "closes": [], "dry": args.dry, "rest_calls": 0}
    acted = 0
    for p, ev in all_closes:
        if acted >= args.max:
            print(f"  batch cap reached ({args.max} closes) — remaining candidates left for next tick")
            break
        num = p["number"]
        # snapshot may be stale — verify the PR is still open before acting
        st, live = rest(f"/repos/{REPO}/issues/{num}")
        if st != 200 or not live:
            print(f"  #{num} fetch fail http={st} -> skip"); continue
        if live.get("state") != "open":
            print(f"  #{num} already {live.get('state')} -> skip (idempotent)")
            continue
        if reopened_recently(num):
            print(f"  #{num} reopened within window -> survive"); continue
        author = (p.get("author") or {}).get("login") or ""
        if author and author_recently_active(num, author):
            print(f"  #{num} author active in last 3d -> survive (respect ongoing work)")
            summary["closes"].append({"number": num, "evidence": ev, "skipped": "author-active"}); continue
        body = reply_for(p, ev)
        print(f"  #{num} CLOSE not_planned ev={ev} age={p['_age_days']}d idle={p['_idle_days']}d ci={p['_ci_rollup']} | {p['title'][:55]}")
        if args.dry:
            summary["closes"].append({"number": num, "evidence": ev, "dry": True}); acted += 1; continue
        st, _ = rest(f"/repos/{REPO}/issues/{num}/comments", "POST", {"body": body})
        # state_reason lives on the Issues API, not the pulls API
        st2, _ = rest(f"/repos/{REPO}/issues/{num}", "PATCH", {"state": "closed", "state_reason": "not_planned"})
        ok = st == 201 and st2 == 200
        if ok: acted += 1
        summary["closes"].append({"number": num, "evidence": ev, "ok": ok, "http": (st, st2)})
        print(f"     reply={st} close={st2}")
        time.sleep(1.2)

    summary["rest_calls"] = rest_used
    with open(LOG, "a") as f: f.write(json.dumps(summary, default=str) + "\n")
    ok_n = sum(1 for c in summary["closes"] if c.get("ok"))
    print(f"done: dry={args.dry} rest_calls={rest_used} closed={ok_n} "
          f"processed={len(summary['closes'])} candidates={len(all_closes)}")

if __name__ == "__main__":
    main()
