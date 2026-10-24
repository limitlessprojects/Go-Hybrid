#!/usr/bin/env python3
"""triage_bot.py — Go-Hybrid PR triage bot.

Encodes the PR verification protocol used across sessions 9-22 so the backlog
manages itself. Read-only by default; anything that writes to GitHub needs an
explicit flag. The bot NEVER merges and NEVER closes — those stay deliberate
acts (credit-close protocol, wildcard verdict window).

Subcommands:
  scan     snapshot every open PR, classify (wildcard hold / draft / conflicts /
           stale-base / duplicate cluster / tests-included / new arrival /
           overlap-risk), score, write TRIAGE.md + TRIAGE.json.
           --verify K additionally runs the worktree merge gate on the top K
           land-candidates.
  approve  approve action_required CI runs for open PR heads (the chore we used
           to do ~190x per session by hand), with a quota guard (--max).
  labels   ensure the triage:* label palette exists and apply per-PR labels.
  board    print or post the triage board to a GitHub issue (idempotent upsert
           via a body marker).

Stdlib only. Token: --token-file, $GITHUB_TOKEN, or /home/z/my-project/.gh_token.
"""

import argparse
import json
import os
import re
import subprocess
import sys
import time
from concurrent.futures import ThreadPoolExecutor, as_completed
from datetime import datetime, timezone
from pathlib import Path
from urllib.error import HTTPError
from urllib.parse import quote
from urllib.request import Request, urlopen

CONFIG_PATH = Path(__file__).resolve().parent / "triage_config.json"
DEFAULT_TOKEN_FILE = "/home/z/my-project/.gh_token"
BOARD_MARKER = "triage-bot:board:v1"
API = "https://api.github.com"


def log(msg):
    print(msg, flush=True)


class RateGuard(Exception):
    pass


class Gh:
    """Minimal GitHub REST client with retries and a rate-limit guard."""

    def __init__(self, repo, token, min_remaining=120):
        self.repo = repo
        self.token = token
        self.min_remaining = min_remaining
        self.calls = 0
        self.remaining = None

    def req(self, method, path, body=None):
        """Returns (status, parsed_json_or_None). Raises RateGuard on low quota."""
        url = path if path.startswith("http") else API + path
        data = json.dumps(body).encode() if body is not None else None
        last_err = None
        for attempt in range(4):
            r = Request(url, data=data, method=method)
            r.add_header("Authorization", "Bearer " + self.token)
            r.add_header("Accept", "application/vnd.github+json")
            r.add_header("X-GitHub-Api-Version", "2022-11-28")
            r.add_header("User-Agent", "go-hybrid-triage-bot")
            if data is not None:
                r.add_header("Content-Type", "application/json")
            try:
                with urlopen(r, timeout=60) as resp:
                    self.calls += 1
                    rem = resp.headers.get("x-ratelimit-remaining")
                    resource = resp.headers.get("x-ratelimit-resource", "core")
                    if rem is not None and resource == "core":
                        # search/graphql have their own small buckets (30/min);
                        # only the core bucket is worth guarding.
                        self.remaining = int(rem)
                        if self.remaining < self.min_remaining:
                            raise RateGuard(
                                "rate limit low: %s remaining" % self.remaining)
                    raw = resp.read().decode()
                    return resp.status, (json.loads(raw) if raw.strip() else None)
            except HTTPError as e:
                body_txt = ""
                try:
                    body_txt = e.read().decode()[:400]
                except Exception:
                    pass
                if e.code == 404:
                    return 404, None
                # GitHub secondary rate limits surface as 403 with a "rate
                # limit" body, not 429 — retry those with a longer backoff
                # (the window is typically 30-60s). Other 403s are permission
                # errors: fail fast, retrying cannot help.
                is_rate_403 = e.code == 403 and "rate limit" in body_txt.lower()
                if (e.code in (429, 502, 503, 504) or is_rate_403) and attempt < 3:
                    time.sleep([2, 5, 10][attempt] if e.code != 403
                               else [5, 20, 45][attempt])
                    last_err = e
                    continue
                raise RuntimeError("%s %s -> %s %s" % (method, path, e.code, body_txt))
            except RateGuard:
                raise
        raise RuntimeError("%s %s failed after retries: %s" % (method, path, last_err))

    def get(self, path):
        return self.req("GET", path)[1]

    def post(self, path, body):
        return self.req("POST", path, body)[1]

    def patch(self, path, body):
        return self.req("PATCH", path, body)[1]

    def paginate(self, path, max_pages=20, key=None):
        """Follow page links; unwrap dict envelopes like {"workflow_runs": [...]}."""
        out, page = [], 1
        while page <= max_pages:
            sep = "&" if "?" in path else "?"
            _, data = self.req("GET", "%s%spage=%d&per_page=100" % (path, sep, page))
            if isinstance(data, dict):
                if key and key in data:
                    data = data[key]
                elif "items" in data:
                    data = data["items"]
                else:
                    data = [data]
            if not data:
                break
            out.extend(data)
            if len(data) < 100:
                break
            page += 1
        return out


# ---------------------------------------------------------------- helpers

def load_cfg():
    cfg = json.loads(CONFIG_PATH.read_text())
    assert cfg["wildcard_holds"] == sorted(cfg["wildcard_holds"])
    return cfg


def load_token(args):
    if getattr(args, "token_env", None) and os.environ.get(args.token_env):
        return os.environ[args.token_env].strip()
    if getattr(args, "token_file", None):
        p = Path(args.token_file)
    elif os.environ.get("GITHUB_TOKEN"):
        return os.environ["GITHUB_TOKEN"].strip()
    else:
        p = Path(DEFAULT_TOKEN_FILE)
    if p.exists():
        return p.read_text().strip()
    sys.exit("no token: set GITHUB_TOKEN or pass --token-file")


def jaccard(a, b):
    if not a or not b:
        return 0.0
    return len(a & b) / len(a | b)


def age_days(iso, now):
    return round((now - datetime.fromisoformat(iso.replace("Z", "+00:00"))).total_seconds() / 86400, 1)


# ---------------------------------------------------------------- scan

def collect(cfg, gh, args):
    now = datetime.now(timezone.utc)
    prs = gh.paginate("/repos/%s/pulls?state=open&sort=created&direction=desc" % gh.repo)
    log("open PRs: %d" % len(prs))
    main_sha = gh.get("/repos/%s/commits/main" % gh.repo)["sha"]
    log("main: %s" % main_sha[:10])

    # overlap detection via the main-side compare is only worth its cost inside
    # the actionable rebase window; a PR 600 commits behind is a stale-base
    # close-candidate regardless of overlap.
    overlap_window = 100

    def one(pr):
        n = pr["number"]
        full = gh.get("/repos/%s/pulls/%d" % (gh.repo, n))
        files = gh.paginate("/repos/%s/pulls/%d/files?per_page=100" % (gh.repo, n))
        paths = [f["filename"] for f in files]
        pset = set(paths)
        adds = sum(f.get("additions", 0) or 0 for f in files)
        dels = sum(f.get("deletions", 0) or 0 for f in files)

        behind_by, overlap_paths, merge_base = 0, set(), None
        cmp1 = gh.get("/repos/%s/compare/%s...%s" % (gh.repo, main_sha, pr["head"]["sha"]))
        if cmp1:
            behind_by = cmp1.get("behind_by", 0) or 0
            merge_base = (cmp1.get("merge_base_commit") or {}).get("sha")
            if 0 < behind_by <= overlap_window and merge_base:
                cmp2 = gh.get("/repos/%s/compare/%s...%s?per_page=250" % (gh.repo, merge_base, main_sha))
                main_paths = {f["filename"] for f in (cmp2 or {}).get("files", [])}
        # Per-file changed line spans, parsed from the patch we already
        # fetched (no extra API calls). Used to tell a real duplicate
        # from two PRs that merely edit the same file (#979).
        hunk_spans = {}
        for f in files:
            spans = []
            for m in re.finditer(
                r"^@@ -\d+(?:,\d+)? \+(\d+)(?:,(\d+))? @@", f.get("patch") or "", re.M
            ):
                start = int(m.group(1))
                count = int(m.group(2)) if m.group(2) is not None else 1
                spans.append((start, start + max(count, 1)))
            hunk_spans[f["filename"]] = spans

        return {
            "number": n,
            "title": pr["title"][:90],
            "author": (pr.get("user") or {}).get("login", "unknown"),
            "created_at": pr["created_at"],
            "updated_at": pr["updated_at"],
            "age_days": age_days(pr["created_at"], now),
            "idle_days": age_days(pr["updated_at"], now),
            "draft": bool(pr.get("draft")),
            "mergeable": full.get("mergeable"),
            "mergeable_state": full.get("mergeable_state"),
            "head_sha": pr["head"]["sha"][:12],
            "files": len(paths),
            "paths_sample": sorted(paths)[:12],
            "additions": adds,
            "deletions": dels,
            "behind_by": behind_by,
            "overlap_risk": sorted(overlap_paths)[:8],
            "tests_included": any(
                "tests/" in p or "/corpus" in p or p.endswith("_reject.hy") or p.endswith("_accept.hy")
                for p in paths),
            "hunk_spans": hunk_spans,
        }

    items = []
    workers = getattr(args, "jobs", 6)
    with ThreadPoolExecutor(max_workers=workers) as ex:
        futs = {ex.submit(one, pr): pr["number"] for pr in prs}
        for fut in as_completed(futs):
            it = fut.result()
            items.append(it)
            if len(items) % 20 == 0 or len(items) == len(prs):
                log("  ... %d/%d scanned" % (len(items), len(prs)))
    items.sort(key=lambda x: -x["number"])
    for it in items:
        log("  #%d %s — behind %d, mergeable %s, +%d/%d files %d" % (
            it["number"], it["mergeable_state"], it["behind_by"], it["mergeable"],
            it["additions"], it["deletions"], it["files"]))
    return items, main_sha




def classify(cfg, items):
    """Sets bucket/score/dup_of/labels on each item; returns (items, clusters)."""
    W = set(cfg["wildcard_holds"])
    D = set(cfg["drafts_parked"])

    # duplicate clusters: union-find over file-set Jaccard
    parent = list(range(len(items)))

    def find(x):
        while parent[x] != x:
            parent[x] = parent[parent[x]]
            x = parent[x]
        return x

    sets = [set(i["paths_sample"]) for i in items]  # sample keeps it cheap; sample=12 paths
    spans = [i.get("hunk_spans") or {} for i in items]

    def shares_changed_lines(a, b):
        """True when the two change-sets touch at least one identical line.

        File overlap alone is not evidence of a duplicate: nearly every
        PR here touches one crate source file plus that crate's
        tests.rs, so two two-file PRs score Jaccard 1.0 on paths while
        editing unrelated code (#608 edits stmt.rs:1034, #786 edits
        stmt.rs:1773). Requiring one shared *line* keeps real duplicates
        and drops shape-only pairs. Falls back to the path test when a
        patch is unavailable, so a missing patch can never hide a real
        duplicate (#979).
        """
        sa, sb = spans[a], spans[b]
        if not sa or not sb:
            return True
        for path, ra in sa.items():
            rb = sb.get(path)
            if not rb:
                continue
            for lo1, hi1 in ra:
                for lo2, hi2 in rb:
                    if min(hi1, hi2) > max(lo1, lo2):
                        return True
        return False

    for a in range(len(items)):
        for b in range(a + 1, len(items)):
            if not sets[a] or not sets[b]:
                continue
            inter = len(sets[a] & sets[b])
            union = len(sets[a] | sets[b])
            if union and inter / union >= cfg["dup_jaccard"]:
                if shares_changed_lines(a, b):
                    parent[find(a)] = find(b)
    clusters = {}
    for i, it in enumerate(items):
        clusters.setdefault(find(i), []).append(it)
    for members in clusters.values():
        if len(members) < 2:
            continue
        # BOUNTY.md: "the first reporter takes the payout". Earliest
        # filed wins, not the biggest diff - a later, larger PR must
        # not inherit the original's slot (#979).
        members.sort(key=lambda x: (x["created_at"], x["number"]))
        primary = members[0]
        for m in members[1:]:
            m["dup_of"] = primary["number"]
            primary.setdefault("dup_members", []).append(m["number"])

    for it in items:
        n, lab = it["number"], []
        it["wildcard_hold"] = n in W
        it["draft_parked"] = it["draft"] or n in D
        s = 0
        if it["tests_included"]:
            s += 3
            lab.append("triage:tests")
        if it["mergeable"] is True:
            s += 2
        if it["mergeable"] is False:
            s -= 2
        if not it["wildcard_hold"] and not it["draft_parked"]:
            s += 2
        if it["additions"] < 400:
            s += 1
        if it["additions"] > cfg["mega_additions"]:
            s -= 1
        if it["behind_by"] == 0:
            s += 1
        if it["behind_by"] >= cfg["behind_threshold"]:
            s -= 1
        if it.get("dup_of"):
            s -= 1
        it["score"] = s

        if it["wildcard_hold"]:
            it["bucket"] = "PARKED-WILDCARD"
            lab.append("triage:wildcard-hold")
        elif it["draft_parked"]:
            it["bucket"] = "PARKED-DRAFT"
        elif it["mergeable"] is False:
            it["bucket"] = "CONFLICTS"
            lab.append("triage:conflicts")
        elif it["behind_by"] >= cfg["behind_threshold"]:
            it["bucket"] = "STALE-BASE"
            lab.append("triage:behind")
        elif it.get("dup_of"):
            it["bucket"] = "DUP"
            lab.append("triage:duplicate")
        elif s >= 6:
            it["bucket"] = "LAND-CANDIDATE"
            lab.append("triage:pareto")
        else:
            it["bucket"] = "RECON"
        if it["age_days"] <= cfg["new_arrival_days"]:
            lab.append("triage:new")
            it["bucket_new"] = True
        if it.get("overlap_risk"):
            lab.append("triage:behind")
        it["labels"] = sorted(set(lab))

        # recommended action
        if it["wildcard_hold"]:
            it["action"] = "PARK (M1 wildcard hold — verdict 2026-11-01, do not merge)"
        elif it["draft_parked"]:
            it["action"] = "PARK (draft — leave with author)"
        elif it["mergeable"] is False:
            it["action"] = "Rebase or close-with-thanks (conflicts vs main)"
        elif it["behind_by"] >= cfg["behind_threshold"]:
            it["action"] = "Ask author to update branch (behind main by %d)" % it["behind_by"]
        elif it.get("dup_of"):
            it["action"] = "DUP of #%d — verify both takes, land strongest, credit both" % it["dup_of"]
        elif it["bucket"] == "LAND-CANDIDATE":
            it["action"] = "Verify (merge gate -> repro -> corpus -> differential) then land"
        else:
            it["action"] = "Recon: read diff, check for subsumption/real content"
    return items, [sorted(m["number"] for m in v) for v in clusters.values() if len(v) > 1]


def merge_gate(items, main_sha, top_k):
    """Worktree merge gate on the top-K land-candidates. Returns per-PR verdicts."""
    cands = sorted([i for i in items if i["bucket"] == "LAND-CANDIDATE"],
                   key=lambda x: (-x["score"], x["additions"]))[:top_k]
    repo = Path(__file__).resolve().parent.parent
    verdicts = {}
    for it in cands:
        n, wt = it["number"], "/tmp/tb-wt-%d" % it["number"]
        try:
            subprocess.run(["git", "-C", str(repo), "fetch", "-q", "origin",
                            "+refs/pull/%d/head" % n], check=True, timeout=120)
            subprocess.run(["git", "-C", str(repo), "worktree", "add", "--detach", wt,
                            "FETCH_HEAD"], check=True, capture_output=True, timeout=60)
            r = subprocess.run(["git", "-C", wt, "merge", "--no-edit", "--no-ff", main_sha],
                               capture_output=True, text=True, timeout=120)
            out = r.stdout + r.stderr
            if r.returncode == 0:
                verdicts[n] = "MERGE-CLEAN"
            elif "CONFLICT" in out or "Automatic merging failed" in out:
                verdicts[n] = "MERGE-CONFLICT"
            else:
                verdicts[n] = "MERGE-ERROR: " + out.strip().splitlines()[-1][:120]
        except subprocess.CalledProcessError as e:
            verdicts[n] = "SETUP-FAIL: " + (e.stderr or "")[-160:]
        finally:
            subprocess.run(["git", "-C", str(repo), "worktree", "remove", "--force", wt],
                           capture_output=True, timeout=60)
    subprocess.run(["git", "-C", str(repo), "worktree", "prune"], capture_output=True)
    for it in items:
        if it["number"] in verdicts:
            it["merge_gate"] = verdicts[it["number"]]
    return verdicts


def render_board(cfg, items, clusters, verdicts, now_s):
    buckets = {}
    for it in items:
        buckets.setdefault(it["bucket"], []).append(it)
    cand = sorted(buckets.get("LAND-CANDIDATE", []), key=lambda x: (-x["score"], x["additions"]))
    pareto = cand[:cfg["pareto_n"]]
    L = ["# Triage Board", "", "Generated %s by `tools/triage_bot.py scan` — review-first automation." % now_s,
         "The bot never merges and never closes. Buckets: %s." % ", ".join(
             "`%s` x%d" % (k, len(v)) for k, v in sorted(buckets.items())),

         ""]
    if verdicts:
        L += ["## Merge gate (top land-candidates, worktree merge vs main)", ""]
        L += ["| PR | verdict |", "|---|---|"]
        L += ["| #%d | %s |" % (n, v) for n, v in sorted(verdicts.items())]
        L.append("")

    def table(rows, note=None):
        out = []
        if not rows:
            if note:
                out += [note, ""]
            return out
        out += ["| PR | title | author | age | +lines | files | tests | behind | score | action |",
                "|---|---|---|---|---|---|---|---|---|---|"]
        for it in rows:
            out.append("| #%d | %s | %s | %sd | +%d | %d | %s | %d | %d | %s |" % (
                it["number"], it["title"].replace("|", "\\|"), it["author"], it["age_days"],
                it["additions"], it["files"], "Y" if it["tests_included"] else "-",
                it["behind_by"], it["score"], it["action"]))
        out.append("")
        return out

    L += ["## Pareto NOW (top %d)" % cfg["pareto_n"], ""]
    L += table(pareto, "_none this pass — run recon on the RECON bucket_")
    L += ["## Land-candidates (rest)", ""]
    L += table(cand[cfg["pareto_n"]:], "_none_")
    L += ["## Conflicts vs main", ""]
    L += table(buckets.get("CONFLICTS", []), "_none_")
    L += ["## Stale base (behind main >= %d)" % cfg["behind_threshold"], ""]
    L += table(buckets.get("STALE-BASE", []), "_none_")
    L += ["## Duplicate clusters (verify both takes, land strongest, credit both)", ""]
    if clusters:
        L += ["- " + ", ".join("#%d" % n for n in c) for c in clusters]
        L.append("")
    else:
        L += ["_none_", ""]
    new = [i for i in items if i.get("bucket_new")]
    L += ["## New arrivals (<= %dd)" % cfg["new_arrival_days"], ""]
    L += table(new, "_none_")
    wl = [i for i in items if i["wildcard_hold"]]
    L += ["## Wildcard holds (M1 — do NOT merge before 2026-11-01 verdicts)", ""]
    L += ["- " + ", ".join("#%d" % i["number"] for i in sorted(wl, key=lambda x: x["number"]))
          + ("  (%d PRs)" % len(wl) if wl else ""), ""]
    L += ["## Recon queue (everything else)", ""]
    L += table(buckets.get("RECON", []), "_none_")
    L += ["<!-- %s -->" % BOARD_MARKER]
    return "\n".join(L), pareto


def cmd_scan(args):
    cfg = load_cfg()
    gh = Gh(cfg["repo"], load_token(args), min_remaining=150)
    items, main_sha = collect(cfg, gh, args)
    items, clusters = classify(cfg, items)
    verdicts = {}
    if args.verify:
        verdicts = merge_gate(items, main_sha, args.verify)
        log("merge gate: %s" % verdicts)
    now_s = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M UTC")
    md, pareto = render_board(cfg, items, clusters, verdicts, now_s)
    out = Path(args.out)
    out.mkdir(parents=True, exist_ok=True)
    (out / "TRIAGE.md").write_text(md)
    (out / "TRIAGE.json").write_text(json.dumps(
        {"generated": now_s, "main": main_sha, "clusters": clusters, "items": items},
        indent=1, ensure_ascii=False))
    log("\nwrote %s/TRIAGE.md and TRIAGE.json (%d API calls, %s remaining)" %
        (out, gh.calls, gh.remaining))
    for it in pareto:
        log("  PARETO: #%-4d score %d  %s" % (it["number"], it["score"], it["title"][:70]))


# ---------------------------------------------------------------- approve

def cmd_approve(args):
    cfg = load_cfg()
    gh = Gh(cfg["repo"], load_token(args))
    prs = gh.paginate("/repos/%s/pulls?state=open&per_page=100" % gh.repo)
    heads = {pr["head"]["sha"]: pr["number"] for pr in prs}
    runs = gh.paginate("/repos/%s/actions/runs?event=pull_request&status=action_required&per_page=100" % gh.repo,
                       key="workflow_runs")
    todo, seen = [], set()
    for r in runs:
        sha = r.get("head_sha", "")
        if sha in heads and sha not in seen:
            seen.add(sha)
            todo.append((r["id"], heads[sha], r["name"]))
    log("action_required runs for open PR heads: %d (cap %s)" % (len(todo), args.max))
    ok = 0
    for run_id, n, name in todo[:args.max]:
        if args.dry:
            log("  DRY would approve: #%d %s (%d)" % (n, name, run_id))
            continue
        try:
            gh.req("POST", "/repos/%s/actions/runs/%d/approve" % (gh.repo, run_id))
            ok += 1
            log("  approved: #%d %s" % (n, name))
        except (RuntimeError, RateGuard) as e:
            log("  FAIL #%d: %s" % (n, e))
            if isinstance(e, RateGuard):
                break
    log("approved %d/%d (%d calls used, %s remaining)" % (ok, len(todo), gh.calls, gh.remaining))


# ---------------------------------------------------------------- labels

def cmd_labels(args):
    cfg = load_cfg()
    gh = Gh(cfg["repo"], load_token(args))
    for name, meta in cfg["labels"].items():
        if args.dry:
            continue
        try:
            gh.post("/repos/%s/labels" % gh.repo,
                    {"name": name, "color": meta["color"], "description": meta["description"]})
        except RuntimeError as e:
            if "422" not in str(e):
                log("  label %s: %s" % (name, e))
    log("palette ensured (%d calls)" % gh.calls)
    items = json.loads(Path(args.from_scan).read_text())["items"] if args.from_scan else None
    if items is None:
        items, _ = classify(cfg, collect(cfg, gh, args)[0])
    for it in items:
        want = it["labels"]
        if not want:
            continue
        if args.dry:
            log("  #%d -> %s" % (it["number"], ",".join(want)))
            continue
        try:
            gh.post("/repos/%s/issues/%d/labels" % (gh.repo, it["number"]), {"labels": want})
        except RuntimeError as e:
            log("  FAIL #%d: %s" % (it["number"], e))
    log("labels applied (%d calls total, %s remaining)" % (gh.calls, gh.remaining))


# ---------------------------------------------------------------- board

def cmd_board(args):
    cfg = load_cfg()
    pareto_n = args.pareto or cfg["pareto_n"]
    cfg = dict(cfg, pareto_n=pareto_n)
    gh = Gh(cfg["repo"], load_token(args), min_remaining=150)
    if args.from_scan:
        d = json.loads(Path(args.from_scan).read_text())
        items, clusters, now_s = d["items"], d["clusters"], d["generated"]
    else:
        items, main_sha = collect(cfg, gh, args)
        items, clusters = classify(cfg, items)
        now_s = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M UTC")
    verdicts = {}
    for it in items:
        if it.get("merge_gate"):
            verdicts[it["number"]] = it["merge_gate"]
    md, pareto = render_board(cfg, items, clusters, verdicts, now_s)
    if not args.post:
        print(md)
        return
    _, srch = gh.req("GET", "/search/issues?q=%s" % quote(
        'repo:%s is:issue is:open "%s" in:body' % (cfg["repo"], BOARD_MARKER)))
    hits = (srch or {}).get("items", [])
    if hits:
        n = hits[0]["number"]
        gh.patch("/repos/%s/issues/%d" % (cfg["repo"], n), {"body": md})
        log("board updated on #%d (%d calls, %s remaining)" % (n, gh.calls, gh.remaining))
    else:
        iss = gh.post("/repos/%s/issues" % cfg["repo"],
                      {"title": "Triage Board (auto-updated daily — pin me)",
                       "body": md})
        log("board created: %s (%d calls)" % (iss.get("html_url"), gh.calls))


# ---------------------------------------------------------------- main

def main():
    ap = argparse.ArgumentParser(description=__doc__,
                                 formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--token-file", default=None)
    ap.add_argument("--token-env", default=None, help="read token from this env var")
    sub = ap.add_subparsers(dest="cmd", required=True)

    p = sub.add_parser("scan", help="snapshot + classify + report")
    p.add_argument("--out", default="/tmp/triage")
    p.add_argument("--verify", type=int, default=0, metavar="K",
                   help="run worktree merge gate on top-K land-candidates")
    p.add_argument("--jobs", type=int, default=6, help="parallel fetch workers")
    p.set_defaults(fn=cmd_scan)

    p = sub.add_parser("approve", help="approve action_required CI runs")
    p.add_argument("--max", type=int, default=None)
    p.add_argument("--dry", action="store_true")
    p.set_defaults(fn=cmd_approve)

    p = sub.add_parser("labels", help="ensure + apply triage labels")
    p.add_argument("--apply", action="store_true")
    p.add_argument("--from-scan", default=None, help="reuse TRIAGE.json instead of re-fetching")
    p.add_argument("--jobs", type=int, default=6, help="parallel fetch workers")
    p.set_defaults(fn=cmd_labels)

    p = sub.add_parser("board", help="print or post the triage board")
    p.add_argument("--post", action="store_true")
    p.add_argument("--pareto", type=int, default=None)
    p.add_argument("--jobs", type=int, default=6, help="parallel fetch workers")
    p.add_argument("--from-scan", default=None, help="reuse TRIAGE.json instead of re-fetching")
    p.set_defaults(fn=cmd_board)

    args = ap.parse_args()
    if args.cmd == "approve" and args.max is None:
        args.max = load_cfg()["approve_max_per_run"]
    if args.cmd == "labels":
        args.dry = not args.apply
    args.fn(args)


if __name__ == "__main__":
    main()
