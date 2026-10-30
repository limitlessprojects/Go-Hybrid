#!/usr/bin/env python3
"""shepherd.py — Go-Hybrid Shepherd: the automation layer on top of triage_bot.

The bot absorbs every repetitive step (board refresh, rebase, cheap gates, dup
cleanup, verify mechanics, structured logging). Humans keep only the judgment
calls: stale-premise closes (#804 class) and real-conflict extraction (#620
class). triage_bot.classify() stays the prior — everything here is layered on
top of it and nothing here replaces the verify protocol.

Kill switch — SHEPHERD_MODE repo variable (unset => dry-run):
  off      every workflow exits at the gate.
  dry-run  read + comment + label only. No merges, no branch updates, no closes.
  live     the narrow auto-mutations unlock (W2 update-branch, W4 Phase-B closes,
           W5 merge) — each still individually flag-gated.

Subcommands:
  gate          workflow kill-switch check (--require read|dry-run|live)
  labels        ensure the bot:* palette exists
  sync-labels   apply structural bot:* labels from a TRIAGE.json (classify's view)
  rebase        W2 rebase shepherd (CAS update-branch, capped, resumable)
  dup           W4 duplicate-cluster closer (Phase A propose / Phase B close)
  docs-resolve  W5 helper: resolve docs-only conflicts (keep main's docs, PR's code)
  probe         W6 stale-premise probe (compile claimed premises vs pinned toolchain)
  verify-assert W5 push-and-assert: PR head must be an ancestor of origin/main
  merge-msg     W5 helper: write the credit merge subject/body files
  comment       marker-deduplicated living comment (edit-in-place per marker)
  test-check    W3 helper: flag PRs with no test/corpus files in the diff
  log           W7 structured session log (JSONL append via contents API)

Stdlib only. Reuses triage_bot.Gh / collect / classify / load_token.
Every bot action leaves a comment or a label; the log is the audit trail.
"""

import argparse
import base64
import json
import os
import re
import subprocess
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import triage_bot as tb  # reuse Gh, load_token, log — the prior, not a replacement

CFG_PATH = Path(__file__).resolve().parent / "shepherd_config.json"
SUMMARY = os.environ.get("GITHUB_STEP_SUMMARY")

MODES = {"off": 0, "dry-run": 1, "live": 2}
# requirement floor per gate level: "read" = anything above off (comment/label
# workflows run at dry-run); "dry-run" = same floor, self-documenting; "live"
# = only live mutates (merge, update-branch, close).
REQUIRE = {"read": 1, "dry-run": 1, "live": 2}

TEST_PATTERNS = ("tests/", "/corpus", "_accept.hy", "_reject.hy",
                 "_test.go", "_test.rs", "test_")

# W6 stale-premise registry. Each rule: a diff pattern that signals the PR
# CLAIMS a rejection, plus a probe that the pinned toolchain either accepts
# (premise is dead -> STALE-PREMISE) or still rejects (premise holds).
# The #804 lesson: Go 1.26.9 started accepting make(<-chan int, 1); a PR built
# on the old behavior would over-reject buildable programs. Over-rejecting is
# a regression — the bot blocks and supplies evidence, a human signs the close.
PROBES = [
    {
        "id": "make-directional-buffered",
        "pattern": r"make\(\s*<-\s*chan",
        "claimed": "HYB must reject make() on a receive-directional channel "
                   "(premise: upstream Go rejects it)",
        "probe": "package main\n\nfunc main() {\n\t_ = make(<-chan int, 1)\n}\n",
    },
]


def load_cfg():
    return json.loads(CFG_PATH.read_text())


def current_mode():
    m = (os.environ.get("SHEPHERD_MODE") or "dry-run").strip().lower()
    return m if m in MODES else "dry-run"


def mode_allows(mode, require):
    """Pure kill-switch logic — does `mode` satisfy `require`?"""
    return MODES.get(mode, MODES["dry-run"]) >= REQUIRE[require]


def step_summary(text):
    if SUMMARY:
        try:
            with open(SUMMARY, "a") as f:
                f.write(text.rstrip() + "\n")
        except OSError:
            pass


def run_url():
    if os.environ.get("GITHUB_RUN_ID"):
        return "%s/%s/actions/runs/%s" % (
            os.environ.get("GITHUB_SERVER_URL", "https://github.com"),
            os.environ.get("GITHUB_REPOSITORY", ""),
            os.environ.get("GITHUB_RUN_ID"))
    return "local"


# ------------------------------------------------------------------ W0 gate

def cmd_gate(args):
    mode = current_mode()
    ok = mode_allows(mode, args.require)
    tb.log("shepherd gate: mode=%s require=%s -> %s"
           % (mode, args.require, "ALLOW" if ok else "BLOCK"))
    if not ok:
        step_summary("Shepherd gate **blocked**: SHEPHERD_MODE=%s does not allow %s"
                     % (mode, args.require))
    sys.exit(0 if ok else 1)


# ------------------------------------------------------------------ labels

SYNC_MAPPING = {
    "LAND-CANDIDATE": ["bot:land-candidate", "triage:pareto"],
    "CONFLICTS": ["bot:conflict"],
    "STALE-BASE": ["bot:stale-base"],
}


def labels_for_item(it):
    """Compute the labels to sync for a given triage item."""
    if it.get("wildcard_hold") or it.get("draft_parked"):
        return []
    bucket = it.get("bucket")
    want = SYNC_MAPPING.get(bucket)
    if not want:
        return []
    return list(want) if isinstance(want, list) else [want]


def ensure_palette(gh, cfg, extra=None):
    """Create any missing bot:* labels (422 = already exists, fine)."""
    labels = dict(cfg["bot_labels"])
    labels.setdefault("triage:pareto", {"color": "5319e7",
                                        "description": "Top-ranked land candidate for the next session"})
    for name in (extra or []):
        labels.setdefault(name, {"color": "cccccc",
                                 "description": "Shepherd W4: superseded by cluster survivor"})
    for name, meta in labels.items():
        try:
            gh.post("/repos/%s/labels" % gh.repo,
                    {"name": name, "color": meta["color"],
                     "description": meta["description"]})
        except RuntimeError as e:
            if "422" not in str(e):
                tb.log("  label %s: %s" % (name, e))


def cmd_labels(args):
    cfg = load_cfg()
    gh = tb.Gh(cfg["repo"], tb.load_token(args))
    ensure_palette(gh, cfg)
    tb.log("bot palette ensured (%d calls)" % gh.calls)


def cmd_sync_labels(args):
    """Structural bot:* labels, straight from classify()'s buckets (plan §2:
    state lives in labels; the board is a view). Idempotent POST per PR."""
    cfg = load_cfg()
    gh = tb.Gh(cfg["repo"], tb.load_token(args))
    ensure_palette(gh, cfg)
    items = json.loads(Path(args.from_scan).read_text())["items"]
    n = 0
    for it in items:
        want_labels = labels_for_item(it)
        if not want_labels:
            continue
        n += 1
        if args.dry:
            tb.log("  #%d -> %s" % (it["number"], ",".join(want_labels)))
            continue
        try:
            gh.post("/repos/%s/issues/%d/labels" % (gh.repo, it["number"]),
                    {"labels": want_labels})
        except (RuntimeError, tb.RateGuard) as e:
            tb.log("  FAIL #%d: %s" % (it["number"], e))
            if isinstance(e, tb.RateGuard):
                break
    tb.log("sync-labels: %d candidates, %d calls, %s remaining"
           % (n, gh.calls, gh.remaining))


# ------------------------------------------------------------------ W2 rebase

def cmd_rebase(args):
    cfg = load_cfg()
    rc = cfg["rebase"]
    gh = tb.Gh(cfg["repo"], tb.load_token(args))
    main_sha = gh.get("/repos/%s/commits/main" % gh.repo)["sha"]
    prs = gh.paginate("/repos/%s/pulls?state=open&per_page=100" % gh.repo)
    exempt = set(rc["exempt_labels"])

    cands = []
    for pr in prs:
        labels = {l["name"] for l in pr.get("labels", [])}
        if pr.get("draft") or (labels & exempt):
            continue
        cands.append(pr)
    # oldest first: the most-behind branches are the most-stale ones
    cands.sort(key=lambda p: p["created_at"])
    tb.log("after label filter: %d candidates (scanning oldest %d)"
           % (len(cands), rc["scan_window"]))

    scored = []
    for pr in cands[:rc["scan_window"]]:
        n = pr["number"]
        full = gh.get("/repos/%s/pulls/%d" % (gh.repo, n))
        if full.get("mergeable") is not True:
            continue  # conflicted PRs go through the W4/W5 human path, not rebase
        cmp = gh.get("/repos/%s/compare/%s...%s" % (gh.repo, main_sha,
                                                    full["head"]["sha"]))
        behind = (cmp or {}).get("behind_by", 0) or 0
        if behind >= rc["behind_min"]:
            scored.append((behind, n, full["head"]["sha"]))
    scored.sort(reverse=True)
    for behind, n, sha in scored:
        tb.log("  candidate: #%d behind by %d (head %s)" % (n, behind, sha[:10]))

    todo = scored[:rc["max_per_run"]]  # 1/run @ */20 = 3/hour cap
    for behind, n, sha in todo:
        if args.dry:
            tb.log("DRY would update-branch: #%d (behind %d, expected_head_sha %s)"
                   % (n, behind, sha[:10]))
            continue
        # Compare-and-swap: update-branch refuses if head moved since we looked.
        try:
            status, _ = gh.req("PUT", "/repos/%s/pulls/%d/update-branch" % (gh.repo, n),
                               {"expected_head_sha": sha})
            tb.log("  #%d update-branch -> %s (CAS ok)" % (n, status))
        except RuntimeError as e:
            msg = str(e)
            if "was modified" in msg or "422" in msg:
                tb.log("  #%d skipped: branch moved under us (CAS mismatch)" % n)
                continue
            tb.log("  #%d FAILED: %s" % (n, msg))
            try:
                st, labels = gh.req("GET", "/repos/%s/issues/%d/labels?per_page=100"
                                    % (gh.repo, n))
                names = {l["name"] for l in (labels or [])}
                if "bot:conflict" not in names:
                    gh.post("/repos/%s/issues/%d/labels" % (gh.repo, n),
                            {"labels": ["bot:conflict"]})
                    gh.post("/repos/%s/issues/%d/comments" % (gh.repo, n), {"body":
                            "Shepherd W2 could not update this branch automatically "
                            "(behind main by %d). Labeled `bot:conflict` — if the "
                            "conflicts are docs-only (PLAN.md/STATUS.md) W5 resolves "
                            "them deterministically; otherwise this needs a human.\n\n"
                            "```\n%s\n```\n\n<!-- shepherd:rebase-fail -->" % (behind,
                            msg[:600])})
            except (RuntimeError, tb.RateGuard) as e2:
                tb.log("  #%d label/comment failed: %s" % (n, e2))
    step_summary("W2 rebase: %d candidate(s) behind>=%d, %d attempted this tick"
                 % (len(scored), rc["behind_min"], len(todo)))


# ------------------------------------------------------------------ W4 dup

def plan_phase_a(items, clusters, is_open):
    """Pure planner: [(survivor, [member,...])] eligible for Phase-A proposals.

    Skips: clusters whose survivor is closed/missing, wildcard-hold or draft
    survivors/members (those have their own verdict windows), and members that
    are themselves parked. The file-set Jaccard signal is a HEURISTIC — the
    comment says so; a verdict never comes from this plan alone.
    """
    by = {i["number"]: i for i in items}
    plan = []
    for cl in sorted(clusters):
        primary = next((by[n] for n in cl
                        if by.get(n) and by[n].get("dup_members")), None)
        if primary is None or not is_open(primary["number"]):
            continue
        if primary.get("wildcard_hold") or primary.get("draft_parked"):
            continue
        members = [by[n] for n in cl
                   if n != primary["number"] and by.get(n)
                   and not by[n].get("wildcard_hold")
                   and not by[n].get("draft_parked")]
        if members:
            plan.append((primary["number"],
                         sorted(m["number"] for m in members)))
    return plan


def _pr_open(gh, n):
    try:
        _, pr = gh.req("GET", "/repos/%s/pulls/%d" % (gh.repo, n))
        return bool(pr and pr.get("state") == "open")
    except RuntimeError:
        return False


def _has_label(gh, n, name):
    _, labels = gh.req("GET", "/repos/%s/issues/%d/labels?per_page=100"
                       % (gh.repo, n))
    return name in {l["name"] for l in (labels or [])}


def _label_age_days(gh, n, name, now):
    _, events = gh.req("GET", "/repos/%s/issues/%d/events?per_page=100"
                       % (gh.repo, n))
    newest = None
    for ev in (events or []):
        if ev.get("event") == "labeled" and (ev.get("label") or {}).get("name") == name:
            newest = ev.get("created_at")
    if not newest:
        return None
    return tb.age_days(newest, now)


def _unique_commits(gh, survivor, member):
    """git cherry: commits on member NOT equivalent (patch-id) to anything on
    the survivor's head. Empty '+' list => member carries nothing unique."""
    repo = Path(__file__).resolve().parent.parent
    for ref, n in (("refs/pull/%d/head" % survivor, survivor),
                   ("refs/pull/%d/head" % member, member)):
        r = subprocess.run(["git", "-C", str(repo), "fetch", "-q", "origin",
                            "+%s:refs/shepherd/%d" % (ref, n)],
                           capture_output=True, text=True, timeout=120)
        if r.returncode != 0:
            return ["<fetch-failed: %s>" % (r.stderr or "")[-120:]]
    r = subprocess.run(["git", "-C", str(repo), "cherry",
                        "refs/shepherd/%d" % survivor,
                        "refs/shepherd/%d" % member],
                       capture_output=True, text=True, timeout=60)
    if r.returncode != 0:
        return ["<cherry-failed: %s>" % (r.stderr or "")[-120:]]
    return [ln for ln in r.stdout.splitlines() if ln.startswith("+")]


def cmd_dup(args):
    cfg = load_cfg()
    dc = cfg["dup"]
    gh = tb.Gh(cfg["repo"], tb.load_token(args))
    d = json.loads(Path(args.from_scan).read_text())
    items, clusters = d["items"], d["clusters"]

    survivors = sorted({p for p, _ in plan_phase_a(items, clusters, lambda n: True)})
    open_map = {n: _pr_open(gh, n) for n in survivors}
    plan = plan_phase_a(items, clusters, lambda n: open_map.get(n, False))
    tb.log("phase-A plan: %d cluster(s) eligible" % len(plan))
    for surv, members in plan:
        tb.log("  survivor #%d <- members %s" % (surv, members))

    mode = current_mode()
    closable = []
    for surv, members in plan:
        for m in members:
            lname = "bot:dup-of:#%d" % surv
            if args.dry:
                tb.log("DRY would propose: #%d superseded by #%d" % (m, surv))
                continue
            if _has_label(gh, m, lname):
                tb.log("  #%d already labeled %s" % (m, lname))
            else:
                ensure_palette(gh, cfg, extra=[lname])
                gh.post("/repos/%s/issues/%d/labels" % (gh.repo, m),
                        {"labels": [lname]})
                gh.post("/repos/%s/issues/%d/comments" % (gh.repo, m), {"body":
                        "Shepherd (Phase A — **propose only**): this PR's changed-file "
                        "set overlaps >=0.6 Jaccard with #%d, which carries the larger "
                        "take. Proposing close as superseded.\n\nThis is a proposal, "
                        "not a close — nothing auto-closes while this comment is "
                        "fresh. If this PR has unique content (a different rule, extra "
                        "tests, a different fix), say so or push, and the label comes "
                        "off. Cluster signal is a file-set heuristic; the verdict stays "
                        "human.\n\n<!-- shepherd:dup-propose -->" % surv})
                tb.log("  proposed: #%d superseded by #%d" % (m, surv))
            closable.append((m, surv))

    if not args.phase_b:
        step_summary("W4 dup: %d cluster(s), %d member(s) in Phase-A plan "
                     "(propose-only; Phase B not requested)" % (len(plan), len(closable)))
        return
    if not mode_allows(mode, "live"):
        tb.log("Phase B requires SHEPHERD_MODE=live (current: %s) — stopping." % mode)
        return

    now = datetime.now(timezone.utc)
    for m, surv in closable:
        age = _label_age_days(gh, m, "bot:dup-of:#%d" % surv, now)
        if age is None or age < dc["grace_days"]:
            tb.log("  #%d labeled %.1fd ago (< %d grace) — stays open" % (m, age or 0,
                                                                         dc["grace_days"]))
            continue
        if not _has_label(gh, surv, "bot:verified"):
            tb.log("  #%d survivor #%d lacks bot:verified — stays open" % (m, surv))
            continue
        uniq = _unique_commits(gh, surv, m)
        if uniq:
            tb.log("  #%d carries %d unique commit(s) — stays open" % (m, len(uniq)))
            continue
        author = (gh.get("/repos/%s/pulls/%d" % (gh.repo, m)).get("user") or {}).get(
            "login", "unknown")
        gh.post("/repos/%s/issues/%d/comments" % (gh.repo, m), {"body":
                "Closing as superseded by #%d after %d clean days: the survivor is "
                "`bot:verified`, and a patch-id sweep (`git cherry`) shows no unique "
                "commits left here. Thank you @%s for the take — credit travels with "
                "the landing per the repo's dup policy (verify both takes, land the "
                "strongest, credit both authors).\n\n<!-- shepherd:dup-close -->"
                % (surv, dc["grace_days"], author)})
        gh.req("PATCH", "/repos/%s/issues/%d" % (gh.repo, m), {"state": "closed"})
        tb.log("  CLOSED #%d (superseded by #%d, credit comment left)" % (m, surv))


# ------------------------------------------------------------------ W5 helpers

def cmd_docs_resolve(args):
    """Docs-only conflict policy (#870 lesson): conflicts confined to
    supervisor-owned docs resolve deterministically — keep main's docs, keep
    the PR's code. Direction: run while merging main INTO the PR tree, so
    main's copy is 'theirs'. Any code conflict => exit 2 => human."""
    cfg = load_cfg()
    owned = set(cfg["owned_docs"])
    out = subprocess.run(["git", "status", "--porcelain"],
                         capture_output=True, text=True, check=True).stdout
    unmerged = sorted({ln[3:].strip() for ln in out.splitlines()
                       if ln[:2] in ("UU", "AA", "DU", "UD", "AU", "UA", "DD")})
    if not unmerged:
        tb.log("docs-resolve: no unmerged paths")
        sys.exit(0)
    docs, real = classify_conflicts(unmerged, owned)
    tb.log("docs-resolve: docs=%s real=%s" % (docs, real))
    if real:
        print("REAL-CONFLICTS: %s" % ", ".join(real))
        step_summary("W5: **real conflicts** need a human: %s" % ", ".join(real))
        sys.exit(2)
    for p in docs:
        subprocess.run(["git", "checkout", "--%s" % args.docs_side, "--", p],
                       check=True)
        subprocess.run(["git", "add", "--", p], check=True)
    print("DOCS-RESOLVED: %s" % ", ".join(docs))


def classify_conflicts(paths, owned):
    """Pure: split unmerged paths into (owned-docs, real-conflicts)."""
    return ([p for p in paths if p in owned],
            [p for p in paths if p not in owned])


def cmd_verify_assert(args):
    """Push-and-assert (plan §3 principle 3): nothing is 'landed' until the PR
    head is a confirmed ancestor of origin/main. Exit 0 => label bot:landed;
    exit 1 => label bot:human-needed and get loud."""
    repo = args.repo or "."
    r = subprocess.run(["git", "-C", repo, "ls-remote", "origin",
                        "refs/heads/main"], capture_output=True, text=True)
    if r.returncode != 0 or not r.stdout.strip():
        print("ASSERT-FAIL: ls-remote failed: %s" % (r.stderr or "")[-200:])
        sys.exit(1)
    main_sha = r.stdout.split()[0]
    subprocess.run(["git", "-C", repo, "fetch", "-q", "origin", "main"],
                   capture_output=True, timeout=120)
    r2 = subprocess.run(["git", "-C", repo, "merge-base", "--is-ancestor",
                         args.head, "FETCH_HEAD"], capture_output=True)
    if r2.returncode == 0:
        print("LAND-CONFIRMED: %s is an ancestor of origin/main (%s)"
              % (args.head[:12], main_sha[:12]))
        step_summary("W5 push-and-assert: **LAND-CONFIRMED** %s on origin/main %s"
                     % (args.head[:12], main_sha[:12]))
        sys.exit(0)
    print("NOT-LANDED: %s is NOT an ancestor of origin/main (%s) — do not "
          "report this landing as done." % (args.head[:12], main_sha[:12]))
    step_summary("W5 push-and-assert: **NOT-LANDED** — %s missing from origin/main"
                 % args.head[:12])
    sys.exit(1)


def cmd_merge_msg(args):
    cfg = load_cfg()
    gh = tb.Gh(cfg["repo"], tb.load_token(args))
    _, pr = gh.req("GET", "/repos/%s/pulls/%d" % (gh.repo, args.pr))
    if not pr:
        sys.exit("PR #%d not found" % args.pr)
    title = (pr.get("title") or "")[:120]
    author = (pr.get("user") or {}).get("login", "unknown")
    body = pr.get("body") or ""
    fixes = re.findall(r"[fF]ix(?:e[sd]?|ing)?\s+#(\d+)", body)
    fixpart = ("fixes #%s" % fixes[0]) if fixes else "refs #%d" % args.pr
    subject = "Merge pr-%d (%s, %s): %s" % (args.pr, author, fixpart, title)
    trailer = ("Original-author: @%s\nVerified-by: Shepherd W5 (%s)\n"
               "Protocol: merge-gate, repro-main-vs-pr, cargo-workspace, "
               "corpus, census, differential-FULL\n" % (author, run_url()))
    Path(args.subject_file).write_text(subject)
    Path(args.body_file).write_text(trailer)
    print("subject: %s" % subject)


# ------------------------------------------------------------------ W6 probe

def added_lines(diff_text):
    for ln in diff_text.splitlines():
        if ln.startswith("+") and not ln.startswith("+++"):
            yield ln[1:]


def cmd_probe(args):
    """Compile each claimed premise against the pinned toolchain. A probe that
    BUILDS while the PR expects a rejection = stale premise = block auto-merge
    and post evidence. Exit code is always 0; callers grep for STALE-PREMISE."""
    diff = Path(args.diff_file).read_text(errors="replace")
    go_bin = args.go or "go"
    lines = list(added_lines(diff))
    tb.log("probe: %d added line(s), %d rule(s)" % (len(lines), len(PROBES)))
    findings = 0
    for rule in PROBES:
        hit = any(re.search(rule["pattern"], ln) for ln in lines)
        if not hit:
            continue
        tb.log("probe: pattern '%s' matched — compiling probe" % rule["id"])
        with tempfile.TemporaryDirectory() as td:
            src = Path(td) / "probe.go"
            src.write_text(rule["probe"])
            r = subprocess.run([go_bin, "build", "-o", "probe.bin", "probe.go"],
                               cwd=td, capture_output=True, text=True, timeout=120)
        if r.returncode == 0:
            findings += 1
            print("STALE-PREMISE: %s — the pinned toolchain ACCEPTS what this "
                  "PR expects rejected" % rule["id"])
            print("  claimed: %s" % rule["claimed"])
            print("  probe:\n```\n%s```\n" % rule["probe"])
        else:
            print("probe-ok: %s — toolchain still rejects; premise holds (%s)"
                  % (rule["id"], (r.stderr or "").strip().splitlines()[-1][:80]))
    if not findings and not any(re.search(r["pattern"], ln) for ln in lines):
        print("no premise patterns matched in diff")
    step_summary("W6 stale-premise probe: %d finding(s)" % findings)


# ------------------------------------------------------------------ W3 helpers

def cmd_test_check(args):
    files = [ln.strip() for ln in Path(args.files_file).read_text().splitlines()
             if ln.strip()]
    covered = [f for f in files if any(p in f for p in TEST_PATTERNS)]
    if covered:
        print("tests: present — %d changed file(s) match test patterns" % len(covered))
        return
    print("tests: ABSENT — WARN: no test/corpus files in this diff. The bot's "
          "score can reach 6 on such PRs; full verify still gates landing, but "
          "this PR needs explicit repro evidence before it can land.")
    step_summary("W3 test-file check: **ABSENT** — testless PR flagged")


def cmd_comment(args):
    """One living comment per marker per PR — W3 pass/fail updates edit in
    place instead of spamming the thread."""
    cfg = load_cfg()
    gh = tb.Gh(cfg["repo"], tb.load_token(args))
    body = Path(args.body_file).read_text()
    if args.marker:
        body = body.rstrip() + "\n\n<!-- %s -->" % args.marker
    _, comments = gh.req("GET", "/repos/%s/issues/%d/comments?per_page=100"
                         % (gh.repo, args.pr))
    target = None
    if args.marker:
        for c in (comments or []):
            if args.marker in (c.get("body") or ""):
                target = c["id"]
                break
    if target:
        gh.patch("/repos/%s/issues/comments/%d" % (gh.repo, target), {"body": body})
        tb.log("comment updated on #%d (%d)" % (args.pr, target))
    else:
        gh.post("/repos/%s/issues/%d/comments" % (gh.repo, args.pr), {"body": body})
        tb.log("comment posted on #%d" % args.pr)


# ------------------------------------------------------------------ W7 log

def cmd_log(args):
    """Structured session log: one JSON line per bot action, appended to
    logs/SESSION.jsonl on the automation/session-log branch via the contents
    API (CAS-retried). The log is a record, the board is the view.

    Best-effort by design: an audit-log write failure must never fail the
    parent workflow (a down log takes down nothing else)."""
    cfg = load_cfg()
    gh = tb.Gh(cfg["repo"], tb.load_token(args))
    branch, path = cfg["log_branch"], cfg["log_path"]
    rec = {"ts": datetime.now(timezone.utc).strftime("%Y-%m-%dT%H:%M:%SZ"),
           "event": args.event, "pr": args.pr, "verdict": args.verdict,
           "sha": args.sha, "note": args.note, "run": run_url()}
    line = json.dumps(rec, ensure_ascii=False)
    try:
        st, ref = gh.req("GET", "/repos/%s/git/ref/heads/%s" % (gh.repo, branch))
        if st == 404:
            main_sha = gh.get("/repos/%s/commits/main" % gh.repo)["sha"]
            gh.post("/repos/%s/git/refs" % gh.repo,
                    {"ref": "refs/heads/" + branch, "sha": main_sha})
            tb.log("log branch %s created from main" % branch)

        def read_log():
            s, cur = gh.req("GET", "/repos/%s/contents/%s?ref=%s" % (gh.repo, path, branch))
            if s == 404 or cur is None:
                return None, ""
            content = base64.b64decode(cur.get("content") or "").decode(
                "utf-8", errors="replace")
            return cur["sha"], content

        payload_sha, content = read_log()
        new_content = (content if content.endswith("\n") or not content
                       else content + "\n") + line + "\n"
        payload = {"message": "shepherd-log: %s %s" % (args.event, args.pr or ""),
                   "content": base64.b64encode(new_content.encode()).decode(),
                   "branch": branch}
        if payload_sha:
            payload["sha"] = payload_sha
        for attempt in range(3):
            try:
                st2, _ = gh.req("PUT", "/repos/%s/contents/%s" % (gh.repo, path), payload)
                if st2 in (200, 201):
                    tb.log("logged: %s" % line)
                    break
            except RuntimeError:
                pass
            _, cur = gh.req("GET", "/repos/%s/contents/%s?ref=%s"
                            % (gh.repo, path, branch))
            if cur and cur.get("sha"):
                payload["sha"] = cur["sha"]
        else:
            tb.log("LOG-WRITE-FAILED after retries (record kept in this run's log): %s" % line)
    except (RuntimeError, tb.RateGuard) as e:
        tb.log("LOG-WRITE-FAILED (non-fatal, record kept in this run's log): %s | %s"
               % (line, e))
    step_summary("W7 log: `%s`" % line)


# ------------------------------------------------------------------ main

def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--token-file", default=None)
    ap.add_argument("--token-env", default=None, help="read token from this env var")
    sub = ap.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("gate", help="kill-switch check")
    p.add_argument("--require", choices=["read", "dry-run", "live"], default="read")
    p.set_defaults(fn=cmd_gate)

    p = sub.add_parser("labels", help="ensure bot:* palette")
    p.set_defaults(fn=cmd_labels)

    p = sub.add_parser("sync-labels", help="apply structural labels from a scan")
    p.add_argument("--from-scan", required=True)
    p.add_argument("--dry", action="store_true")
    p.set_defaults(fn=cmd_sync_labels)

    p = sub.add_parser("rebase", help="W2 rebase shepherd")
    p.add_argument("--dry", action="store_true")
    p.set_defaults(fn=cmd_rebase)

    p = sub.add_parser("dup", help="W4 duplicate-cluster closer")
    p.add_argument("--from-scan", required=True)
    p.add_argument("--dry", action="store_true",
                   help="rehearsal: print the Phase-A plan, write nothing")
    p.add_argument("--phase-b", action="store_true",
                   help="enable auto-close (needs live mode + bot:verified survivor)")
    p.set_defaults(fn=cmd_dup)

    p = sub.add_parser("docs-resolve", help="W5 docs-only conflict resolution")
    p.add_argument("--docs-side", choices=["theirs", "ours"], default="theirs",
                   help="which side keeps owned docs (theirs = main, when merging "
                        "main INTO the PR tree)")
    p.set_defaults(fn=cmd_docs_resolve)

    p = sub.add_parser("probe", help="W6 stale-premise probe")
    p.add_argument("--diff-file", required=True)
    p.add_argument("--go", default="go", help="go binary to compile probes with")
    p.set_defaults(fn=cmd_probe)

    p = sub.add_parser("verify-assert", help="W5 push-and-assert")
    p.add_argument("--head", required=True, help="PR head SHA")
    p.add_argument("--repo", default=".", help="checkout dir with origin remote")
    p.set_defaults(fn=cmd_verify_assert)

    p = sub.add_parser("merge-msg", help="W5 credit merge message files")
    p.add_argument("--pr", type=int, required=True)
    p.add_argument("--subject-file", default="/tmp/merge-subject.txt")
    p.add_argument("--body-file", default="/tmp/merge-body.txt")
    p.set_defaults(fn=cmd_merge_msg)

    p = sub.add_parser("comment", help="living comment per marker")
    p.add_argument("--pr", type=int, required=True)
    p.add_argument("--marker", default=None)
    p.add_argument("--body-file", required=True)
    p.set_defaults(fn=cmd_comment)

    p = sub.add_parser("test-check", help="W3 test-file presence flag")
    p.add_argument("--files-file", required=True,
                   help="newline-separated changed paths")
    p.set_defaults(fn=cmd_test_check)

    p = sub.add_parser("log", help="W7 structured session log")
    p.add_argument("--event", required=True)
    p.add_argument("--pr", type=int, default=None)
    p.add_argument("--verdict", default=None)
    p.add_argument("--sha", default=None)
    p.add_argument("--note", default=None)
    p.set_defaults(fn=cmd_log)

    args = ap.parse_args()
    args.fn(args)


if __name__ == "__main__":
    main()
