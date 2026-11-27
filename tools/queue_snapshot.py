#!/usr/bin/env python3
"""Snapshot every open PR on limitlessprojects/Go-Hybrid via GraphQL.

Captures per-PR: number, age, last update, author, draft, mergeable,
mergeStateStatus, labels, CI rollup state, check-suite conclusions,
comment count. Writes JSON + prints distributions. Read-only (GraphQL pool).
"""
import json, os, sys, time, urllib.request
from collections import Counter
from datetime import datetime, timezone

REPO_OWNER, REPO_NAME = "limitlessprojects", "Go-Hybrid"
TOKEN = os.environ.get("GH_TOKEN") or os.environ.get("GITHUB_TOKEN") or ""
OUT = os.environ.get("SURVEY_OUT", os.path.join(
    os.environ.get("TRIAGE_STATE_DIR", "/tmp/gh-triage"), "queue_snapshot.json"))

# Non-gate housekeeping workflows (e.g. the daily triage board job) can fail or
# be cancelled on external fork PRs even when every real CI gate (compiler,
# quick, preflight) is green. Exclude them from the effective CI rollup so
# healthy PRs are neither falsely closed nor dropped from land candidates.
NON_GATE_CHECKS = frozenset(
    name.strip().lower()
    for name in os.environ.get("TRIAGE_NON_GATE_CHECKS", "board").split(",")
    if name.strip()
)
FAILING_CONCLUSIONS = frozenset({"FAILURE", "TIMED_OUT", "STARTUP_FAILURE", "ACTION_REQUIRED", "ERROR"})
PASSING_CONCLUSIONS = frozenset({"SUCCESS", "SKIPPED", "NEUTRAL"})

QUERY = """
query($cursor: String) {
  repository(owner: "%s", name: "%s") {
    pullRequests(first: 40, after: $cursor, states: OPEN,
                 orderBy: {field: CREATED_AT, direction: DESC}) {
      pageInfo { hasNextPage endCursor }
      nodes {
        number title createdAt updatedAt isDraft
        author { login }
        mergeable mergeStateStatus
        labels(first: 20) { nodes { name } }
        comments { totalCount }
        commits(last: 1) { nodes { commit {
          statusCheckRollup {
            state
            contexts(first: 50) {
              nodes {
                __typename
                ... on CheckRun { name status conclusion }
                ... on StatusContext { context state }
              }
            }
          }
        } } }
      }
    }
  }
}
""" % (REPO_OWNER, REPO_NAME)


def summarize_ci(rollup, non_gate_checks=NON_GATE_CHECKS):
    """Return (effective_state, failed_gate_concls, ignored_checks).

    When per-check contexts are available, ignore non-gate housekeeping checks
    (such as `board`) so their failure/cancellation cannot poison a green PR's
    CI rollup. Falls back to the raw rollup state only when no contexts were
    recorded (legacy snapshots).
    """
    if not rollup:
        return "NONE", {}, {}
    raw_state = (rollup.get("state") or "NONE").upper()
    contexts = (rollup.get("contexts") or {}).get("nodes")
    if contexts is None:
        return raw_state, {}, {}

    gate_states = []
    failed_concls = {}
    ignored = {}
    for node in contexts:
        if not isinstance(node, dict):
            continue
        typename = node.get("__typename")
        if typename == "StatusContext" or ("context" in node and "name" not in node):
            name = (node.get("context") or "").strip()
            state = (node.get("state") or "").upper()
            if name.lower() in non_gate_checks:
                ignored[name] = state or "UNKNOWN"
                continue
            if state in ("FAILURE", "ERROR"):
                gate_states.append("FAILURE")
                failed_concls[name or "status"] = state
            elif state in ("PENDING", "EXPECTED"):
                gate_states.append("PENDING")
            elif state == "SUCCESS":
                gate_states.append("SUCCESS")
        else:
            name = (node.get("name") or "").strip()
            status = (node.get("status") or "").upper()
            concl = (node.get("conclusion") or "").upper()
            if name.lower() in non_gate_checks:
                ignored[name] = concl or status or "UNKNOWN"
                continue
            if status and status != "COMPLETED":
                gate_states.append("PENDING")
            elif concl in FAILING_CONCLUSIONS:
                gate_states.append("FAILURE")
                failed_concls[name or "check"] = concl
            elif concl == "CANCELLED":
                gate_states.append("CANCELLED")
                failed_concls[name or "check"] = concl
            elif concl in PASSING_CONCLUSIONS:
                gate_states.append("SUCCESS")
            elif concl:
                gate_states.append("PENDING")

    if not gate_states:
        return "NONE", {}, ignored
    if "FAILURE" in gate_states:
        return "FAILURE", failed_concls, ignored
    if "PENDING" in gate_states:
        return "PENDING", failed_concls, ignored
    if "CANCELLED" in gate_states:
        return "CANCELLED", failed_concls, ignored
    return "SUCCESS", {}, ignored

def gql(query, variables):
    body = json.dumps({"query": query, "variables": variables}).encode()
    req = urllib.request.Request("https://api.github.com/graphql", method="POST", data=body,
        headers={"Authorization": f"Bearer {TOKEN}", "User-Agent": "gh-survey",
                 "Content-Type": "application/json"})
    for attempt in range(5):
        try:
            with urllib.request.urlopen(req, timeout=60) as r:
                return json.loads(r.read())
        except urllib.error.HTTPError as e:
            if e.code in (403, 429, 502):
                retry_after = e.headers.get("Retry-After")
                if retry_after: wait = int(retry_after) + 2
                else:
                    reset = e.headers.get("X-RateLimit-Reset")
                    wait = max(20, int(reset) - int(time.time()) + 2) if reset else 120
                print(f"  rate-limited, sleeping {wait}s", flush=True)
                time.sleep(min(wait, 600)); continue
            if e.code >= 500:
                time.sleep(10 * (attempt + 1)); continue
            print(f"  HTTP {e.code}: {e.read()[:300]}", flush=True); raise
        except Exception as ex:
            print(f"  retry after err: {ex}", flush=True); time.sleep(10 * (attempt + 1))
    raise SystemExit("GraphQL failed after retries")

def main():
    if not TOKEN:
        sys.exit("GH_TOKEN missing")
    prs, cursor, pages = [], None, 0
    while True:
        d = gql(QUERY, {"cursor": cursor})
        if d.get("errors"): print("GraphQL errors:", d["errors"][:2]); break
        conn = d["data"]["repository"]["pullRequests"]
        prs.extend(conn["nodes"])
        pages += 1
        print(f"page {pages}: +{len(conn['nodes'])} (total {len(prs)})", flush=True)
        if not conn["pageInfo"]["hasNextPage"]: break
        cursor = conn["pageInfo"]["endCursor"]
        time.sleep(1)

    now = datetime.now(timezone.utc)
    for p in prs:
        p["_age_days"] = round((now - datetime.fromisoformat(p["createdAt"].replace("Z", "+00:00"))).total_seconds() / 86400, 1)
        p["_idle_days"] = round((now - datetime.fromisoformat(p["updatedAt"].replace("Z", "+00:00"))).total_seconds() / 86400, 1)
        cs = p["commits"]["nodes"][0]["commit"] if p["commits"]["nodes"] else None
        rollup = cs.get("statusCheckRollup") if cs else None
        eff_state, failed_concls, ignored_checks = summarize_ci(rollup)
        p["_raw_ci_rollup"] = rollup["state"] if rollup and rollup.get("state") else "NONE"
        p["_ci_rollup"] = eff_state
        p["_suite_concls"] = failed_concls
        p["_ignored_checks"] = ignored_checks
        p["_labels"] = [l["name"] for l in p["labels"]["nodes"]]

    # Session-195 fixup: GitHub computes mergeability lazily, so the bulk GraphQL
    # survey records UNKNOWN for nearly every open PR while individual REST GETs
    # return the real value. Walk the green-CI + non-draft + hold-free subset
    # (the only PRs whose merge state feeds a decision — land candidates) and
    # overwrite the surveyed fields with REST ground truth. Rate limits stop the
    # walk early; affected PRs simply keep their UNKNOWN survey values.
    REST_PROTECTED = {"triage:wildcard-hold", "triage:pareto", "triage:keep"}
    rest_target = [p for p in prs if p["_ci_rollup"] == "SUCCESS" and not p["isDraft"]
                   and not (set(p["_labels"]) & REST_PROTECTED)]
    print(f"\nREST mergeability fixup: {len(rest_target)} green-CI hold-free PRs", flush=True)
    fixed = 0
    for p in rest_target:
        try:
            req = urllib.request.Request(
                f"https://api.github.com/repos/{REPO_OWNER}/{REPO_NAME}/pulls/{p['number']}",
                headers={"Authorization": f"Bearer {TOKEN}", "Accept": "application/vnd.github+json",
                         "User-Agent": "queue-snapshot"})
            with urllib.request.urlopen(req, timeout=30) as r:
                pr = json.loads(r.read().decode())
            p["mergeable"] = "MERGEABLE" if pr.get("mergeable") is True else "CONFLICTING" if pr.get("mergeable") is False else "UNKNOWN"
            p["mergeStateStatus"] = str(pr.get("mergeable_state") or "unknown").upper()
            fixed += 1
            time.sleep(0.5)
        except urllib.error.HTTPError as e:
            if e.code in (403, 429):
                print(f"  rate-limited at #{p['number']} — fixup stops early", flush=True)
                break
            print(f"  #{p['number']}: HTTP {e.code}", flush=True)
        except Exception as ex:
            print(f"  #{p['number']}: {ex}", flush=True)
    print(f"mergeability fixed for {fixed}/{len(rest_target)}", flush=True)

    os.makedirs(os.path.dirname(OUT), exist_ok=True)
    with open(OUT, "w") as f: json.dump(prs, f, indent=1, default=str)

    def dist(items, key, title):
        c = Counter(str(i.get(key)) for i in items)
        print(f"\n{title}")
        for k, v in c.most_common(12): print(f"  {k:35s} {v}")

    print(f"\n=== {len(prs)} open PRs ===")
    dist(prs, "_age_days", "AGE BUCKETS:")
    b = Counter("0-1d" if p["_age_days"] < 1 else "1-3d" if p["_age_days"] < 3 else "3-5d" if p["_age_days"] < 5 else "5-10d" if p["_age_days"] < 10 else "10-21d" if p["_age_days"] < 21 else "21d+" for p in prs)
    for k in ["0-1d", "1-3d", "3-5d", "5-10d", "10-21d", "21d+"]:
        print(f"  {k:8s} {b.get(k, 0)}")
    dist(prs, "_ci_rollup", "CI ROLLUP (GATES ONLY):")
    dist(prs, "mergeable", "MERGEABLE:")
    dist(prs, "mergeStateStatus", "MERGE STATE:")
    prot = [p for p in prs if any(l.startswith("triage:") and l != "triage:new" and l != "triage:conflicts" and l != "triage:behind" and l != "triage:stale" and l != "triage:duplicate" and l != "triage:tests" for l in p["_labels"])]
    print(f"\nprotected-label PRs: {len(prot)}")
    for p in prot[:20]: print(f"  #{p['number']} {p['_labels']}")
    print(f"\ndrafts: {sum(1 for p in prs if p['isDraft'])}")
    old_unprot = [p for p in prs if p["_age_days"] >= 5 and not any(l in ("triage:wildcard-hold", "triage:pareto", "triage:keep") for l in p["_labels"])]
    print(f"age>=5d, unprotected: {len(old_unprot)}")
    ci_fail = [p for p in old_unprot if p["_ci_rollup"] == "FAILURE"]
    ci_ok = [p for p in old_unprot if p["_ci_rollup"] == "SUCCESS"]
    ci_none = [p for p in old_unprot if p["_ci_rollup"] in ("NONE", "EXPECTED")]
    print(f"  of which CI FAILURE: {len(ci_fail)} | CI SUCCESS: {len(ci_ok)} | NONE/EXPECTED: {len(ci_none)}")
    conf = [p for p in prs if p["mergeable"] == "CONFLICTING"]
    print(f"CONFLICTING (any age): {len(conf)}")
    print(f"\nsaved -> {OUT}")


if __name__ == "__main__":
    main()
