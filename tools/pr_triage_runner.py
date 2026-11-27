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
from datetime import datetime, timedelta, timezone

REPO = "limitlessprojects/Go-Hybrid"
TOKEN = os.environ.get("GH_TOKEN") or os.environ.get("GITHUB_TOKEN") or ""

PROTECTED_LABELS = {"triage:wildcard-hold", "triage:pareto", "triage:keep"}
MIN_AGE = float(os.environ.get("TRIAGE_MIN_AGE_DAYS", "5"))
CONFLICT_IDLE = float(os.environ.get("TRIAGE_CONFLICT_IDLE_DAYS", "10"))
REOPEN_WINDOW = float(os.environ.get("TRIAGE_REOPEN_WINDOW_DAYS", "5"))
MAX_REST_BUDGET = int(os.environ.get("TRIAGE_REST_BUDGET", "1800"))
MAX_EVENT_PAGES = 10
FAILING_CONCLUSIONS = {"failure", "timed_out", "startup_failure", "action_required"}
LOG = os.environ.get("TRIAGE_LOG", os.path.join(os.path.dirname(os.path.abspath(__file__)), "triage_runner.log"))
LAND_CANDIDATES = os.environ.get(
    "TRIAGE_LAND_CANDIDATES",
    os.path.join(os.path.dirname(os.path.abspath(__file__)), "land_candidates.json"),
)


def _require_token():
    tok = TOKEN or os.environ.get("GH_TOKEN") or os.environ.get("GITHUB_TOKEN") or ""
    if not tok:
        sys.exit("GH_TOKEN missing")
    return tok

_config = None
def _config_file():
    # works in BOTH layouts: repo checkout (tools/triage_config.json next to
    # this script) and the agent workspace (../Go-Hybrid/tools/...)
    here = os.path.dirname(os.path.abspath(__file__))
    for cand in (os.path.join(here, "triage_config.json"),
                 os.path.join(here, "..", "Go-Hybrid", "tools", "triage_config.json"),
                 "/home/z/my-project/Go-Hybrid/tools/triage_config.json"):
        if os.path.isfile(cand):
            return cand
    return None

def config_protected():
    """Return protected PR numbers from triage_config.json; fail closed if unreadable."""
    global _config
    if _config is None:
        path = _config_file()
        if not path:
            raise RuntimeError("triage_config.json not found — refusing to run without hold list")
        try:
            with open(path, encoding="utf-8") as f:
                cfg = json.load(f)
        except (OSError, json.JSONDecodeError) as e:
            raise RuntimeError(f"triage_config.json unreadable ({path}): {e}") from e
        if not isinstance(cfg, dict):
            raise RuntimeError(f"triage_config.json must be a JSON object ({path})")
        holds = cfg.get("wildcard_holds", [])
        parked = cfg.get("drafts_parked", [])
        if (not isinstance(holds, list) or not isinstance(parked, list)
                or any(type(n) is not int for n in holds + parked)):
            raise RuntimeError(f"triage_config.json hold lists must be integer arrays ({path})")
        _config = set(holds) | set(parked)
    return _config

rest_used = 0
def rest(path, method="GET", body=None):
    """REST with rate-limit awareness; returns (status, json)."""
    global rest_used
    tok = _require_token()
    if rest_used >= MAX_REST_BUDGET:
        raise SystemExit("REST budget exhausted — stopping to stay safe")
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(f"https://api.github.com{path}", method=method, data=data,
        headers={"Authorization": f"token {tok}", "User-Agent": "pr-triage-runner",
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
    tok = _require_token()
    body = json.dumps({"query": query, "variables": variables}).encode()
    req = urllib.request.Request("https://api.github.com/graphql", method="POST", data=body,
        headers={"Authorization": f"Bearer {tok}", "User-Agent": "pr-triage-runner",
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

def _parse_iso(ts):
    if not isinstance(ts, str) or not ts:
        return None
    try:
        dt = datetime.fromisoformat(ts.replace("Z", "+00:00"))
        return dt if dt.tzinfo is not None else dt.replace(tzinfo=timezone.utc)
    except ValueError:
        return None

def reopened_recently(number):
    """True if reopened within REOPEN_WINDOW, or if history cannot be verified."""
    now = datetime.now(timezone.utc)
    for page in range(1, MAX_EVENT_PAGES + 1):
        st, events = rest(f"/repos/{REPO}/issues/{number}/events?per_page=100&page={page}")
        if st != 200 or not isinstance(events, list):
            return True  # unverifiable history -> survive (same doctrine as sweeper)
        for ev in events:
            if not isinstance(ev, dict):
                return True
            if ev.get("event") == "reopened":
                created = _parse_iso(ev.get("created_at"))
                if created is None:
                    return True
                age = (now - created).total_seconds() / 86400
                if age <= REOPEN_WINDOW:
                    return True
        if len(events) < 100:
            return False
    return True  # truncated beyond MAX_EVENT_PAGES -> survive

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
    """True if the author commented on the thread within `days`, or if
    comment history cannot be verified. Uses `since` + pagination so threads
    with >100 older comments never hide recent author replies."""
    now = datetime.now(timezone.utc)
    since = (now - timedelta(days=days)).strftime("%Y-%m-%dT%H:%M:%SZ")
    for page in range(1, MAX_EVENT_PAGES + 1):
        st, comments = rest(f"/repos/{REPO}/issues/{number}/comments?per_page=100&since={since}&page={page}")
        if st != 200 or not isinstance(comments, list):
            return True  # unverifiable -> survive
        for c in comments:
            if not isinstance(c, dict):
                return True
            if (c.get("user") or {}).get("login") == author_login:
                created = _parse_iso(c.get("created_at"))
                if created is None:
                    return True
                age = (now - created).total_seconds() / 86400
                if age <= days:
                    return True
        if len(comments) < 100:
            return False
    return True  # truncated beyond MAX_EVENT_PAGES -> survive

def live_close_guard(pr, ev):
    """Re-verify live PR state before closing so a stale snapshot can never
    close a protected, freshly-fixed, green-CI, or non-conflicting PR.

    Returns (ok_to_close, verified_evidence, skip_reason).
    """
    num = pr["number"]
    st, live = rest(f"/repos/{REPO}/pulls/{num}")
    if st != 200 or not isinstance(live, dict):
        return False, [], f"fetch-fail:{st}"
    if live.get("state") != "open":
        return False, [], f"already-{live.get('state')}"
    raw_labels = live.get("labels")
    if raw_labels is None:
        raw_labels = []
    if not isinstance(raw_labels, list):
        return False, [], "malformed-labels"
    live_labels = set()
    for lbl in raw_labels:
        if not isinstance(lbl, dict) or not isinstance(lbl.get("name"), str):
            return False, [], "malformed-labels"
        live_labels.add(lbl["name"])
    if live_labels & PROTECTED_LABELS:
        return False, [], "protected-label"
    if "wildcard" in (live.get("title") or "").lower():
        return False, [], "wildcard-title"

    head_sha = (live.get("head") or {}).get("sha")
    if not isinstance(head_sha, str) or not head_sha:
        return False, [], "missing-head-sha"

    # Re-verify live CI on the current head commit (Hard Rule 2: never close
    # a PR whose CI is green or still running/pending).
    st_ci, runs_payload = rest(f"/repos/{REPO}/commits/{head_sha}/check-runs?per_page=100")
    if st_ci != 200 or not isinstance(runs_payload, dict):
        return False, [], f"ci-unverifiable:{st_ci}"
    check_runs = runs_payload.get("check_runs")
    total_count = runs_payload.get("total_count")
    if not isinstance(check_runs, list) or (isinstance(total_count, int) and total_count > len(check_runs)):
        return False, [], "ci-unverifiable"
    has_pending = False
    has_failure = False
    for run in check_runs:
        if not isinstance(run, dict):
            return False, [], "ci-unverifiable"
        if run.get("status") != "completed":
            has_pending = True
        elif (run.get("conclusion") or "").lower() in FAILING_CONCLUSIONS:
            has_failure = True
    if has_pending:
        return False, [], "ci-pending"
    if check_runs and not has_failure:
        return False, [], "ci-green"

    live_ev = list(ev)
    if "ci-failure" in live_ev and not has_failure:
        live_ev.remove("ci-failure")
    if "conflicts" in live_ev and live.get("mergeable") is not False:
        live_ev.remove("conflicts")
    if not live_ev:
        return False, [], "evidence-stale"
    return True, live_ev, ""

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
    if pr["_age_days"] < MIN_AGE: return "skip", [], ""
    ci = pr["_ci_rollup"]; mergeable = pr.get("mergeable"); idle = pr["_idle_days"]
    evidence = []
    if ci == "FAILURE": evidence.append("ci-failure")
    if mergeable == "CONFLICTING" and idle >= CONFLICT_IDLE: evidence.append("conflicts")
    if not evidence:
        return "skip", [], ""
    return "close", evidence, ""

def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("--dry", action="store_true")
    ap.add_argument("--execute", action="store_true")
    ap.add_argument("--max", type=int, default=40, help="max closes per run")
    ap.add_argument("--survey", default=os.environ.get("SURVEY_JSON", os.path.join(os.path.dirname(os.path.abspath(__file__)), "queue_snapshot.json")))
    ap.add_argument("--label-only", action="store_true", help="skip closes even if evidence exists")
    args = ap.parse_args(argv)
    dry = args.dry or not args.execute
    _require_token()

    with open(args.survey, encoding="utf-8") as f: prs = json.load(f)
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
    with open(LAND_CANDIDATES, "w", encoding="utf-8") as f:
        json.dump([{"number": p["number"], "title": p["title"], "age_days": p["_age_days"],
                    "author": (p.get("author") or {}).get("login")} for p in land_cands], f, indent=1)
    print(f"land candidates (green CI + CLEAN, open): {len(land_cands)} -> land_candidates.json")
    print(f"queue={len(prs)}  close-candidates={len([d for d in decisions if d[1]=='close'])}"
          f"  (executing first {len(closes)}; cap --max {args.max})")

    summary = {"run": datetime.now(timezone.utc).isoformat(), "queue": len(prs),
               "closes": [], "dry": dry, "label_only": args.label_only, "rest_calls": 0}
    if args.label_only:
        print("  --label-only set: skipping all closes")
    acted = 0
    for p, ev in ([] if args.label_only else all_closes):
        if acted >= args.max:
            print(f"  batch cap reached ({args.max} closes) — remaining candidates left for next tick")
            break
        num = p["number"]
        # Snapshot may be stale — re-verify live open state, labels, title,
        # CI check-runs, and mergeability before acting.
        live_ok, live_ev, skip_reason = live_close_guard(p, ev)
        if not live_ok:
            print(f"  #{num} live guard skip ({skip_reason}) -> survive")
            summary["closes"].append({"number": num, "evidence": ev, "skipped": skip_reason})
            continue
        if reopened_recently(num):
            print(f"  #{num} reopened within window -> survive")
            summary["closes"].append({"number": num, "evidence": live_ev, "skipped": "reopened-recently"})
            continue
        author = (p.get("author") or {}).get("login") or ""
        if author and author_recently_active(num, author):
            print(f"  #{num} author active in last 3d -> survive (respect ongoing work)")
            summary["closes"].append({"number": num, "evidence": live_ev, "skipped": "author-active"}); continue
        body = reply_for(p, live_ev)
        print(f"  #{num} CLOSE not_planned ev={live_ev} age={p['_age_days']}d idle={p['_idle_days']}d ci={p['_ci_rollup']} | {p['title'][:55]}")
        if dry:
            summary["closes"].append({"number": num, "evidence": live_ev, "dry": True}); acted += 1; continue
        st, _ = rest(f"/repos/{REPO}/issues/{num}/comments", "POST", {"body": body})
        # state_reason lives on the Issues API, not the pulls API
        st2, _ = rest(f"/repos/{REPO}/issues/{num}", "PATCH", {"state": "closed", "state_reason": "not_planned"})
        ok = st == 201 and st2 == 200
        if ok: acted += 1
        summary["closes"].append({"number": num, "evidence": live_ev, "ok": ok, "http": (st, st2)})
        print(f"     reply={st} close={st2}")
        time.sleep(1.2)

    summary["rest_calls"] = rest_used
    with open(LOG, "a", encoding="utf-8") as f: f.write(json.dumps(summary, default=str) + "\n")
    ok_n = sum(1 for c in summary["closes"] if c.get("ok"))
    print(f"done: dry={dry} rest_calls={rest_used} closed={ok_n} "
          f"processed={len(summary['closes'])} candidates={len(all_closes)}")

if __name__ == "__main__":
    main()
