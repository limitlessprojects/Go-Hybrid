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
TOKEN = os.environ["GH_TOKEN"]
OUT = os.environ.get("SURVEY_OUT", os.path.join(
    os.environ.get("TRIAGE_STATE_DIR", "/tmp/gh-triage"), "queue_snapshot.json"))

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
          statusCheckRollup { state }
        } } }
      }
    }
  }
}
""" % (REPO_OWNER, REPO_NAME)

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
    p["_ci_rollup"] = cs["statusCheckRollup"]["state"] if cs and cs.get("statusCheckRollup") else "NONE"
    p["_suite_concls"] = {}
    p["_labels"] = [l["name"] for l in p["labels"]["nodes"]]

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
dist(prs, "_ci_rollup", "CI ROLLUP:")
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
