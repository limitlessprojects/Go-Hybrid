#!/usr/bin/env python3
"""autoland.py — autonomous verify-and-land for PREPARED fix bundles.

Closes the loop the sessions author: a fix bundle lives in
`automation/land-queue/<slug>/` as

    meta.json   {"issue": 123, "expected_code": "HYB-Exxxx", "basis": "...", }
    fix.patch   the full fix diff (raw git format: code + tests + fixtures)
    repro.hy    the issue's minimal repro (pre-fix tree must ACCEPT it)

and this pipeline lands it without a human awake:

    1. BASELINE  the unpatched tree must ACCEPT the repro (exit 0). A REJECT
       means the issue is stale or already fixed — never push.
    2. APPLY     `git apply fix.patch`, rebuild, the repro must REJECT with
       the expected code. An ACCEPT means the patch is ineffective — never
       push.
    3. BATTERY   cargo test --workspace, corpus harness, census, differential
       loop (FULL: cargo AND go). Any failure reverts the tree — never push.
    4. LAND      commit (Fixes #N) -> push -> ls-remote ASSERT that origin/main
       contains the landing before anything is reported landed
       (the a4a9dab/b84fff6 incident class is structurally impossible).
    5. RECORD    bundle meta flips to status=landed + sha; shepherd.py log
       appends to logs/SESSION.jsonl on the automation/session-log branch —
       the change log. Failures record equally loudly.

Honesty rules: a bundle that fails any gate is reported BLOCKED with the raw
evidence and lands nothing; dry-run mode (SHEPHERD_MODE != live) executes
nothing but baseline checks and records what WOULD happen. Every exit path
prints a one-line verdict.
"""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import re
import subprocess
import sys
import time

ROOT = pathlib.Path(__file__).resolve().parent.parent
QUEUE = ROOT / "automation" / "land-queue"
HYB = ROOT / "compiler" / "target" / "debug" / "hyb"
COMPILER = ROOT / "compiler"

_SLUG_RE = re.compile(r"^[A-Za-z0-9][A-Za-z0-9._-]*\Z")


def _safe_bundle_dir(slug: str, queue: pathlib.Path | None = None) -> pathlib.Path | None:
    if not isinstance(slug, str) or not _SLUG_RE.fullmatch(slug):
        return None
    if ".." in slug.split("/") or ".." in slug.split("\\"):
        return None
    q = (queue or QUEUE).resolve()
    try:
        d = (q / slug).resolve()
    except OSError:
        return None
    if d == q or q not in d.parents:
        return None
    return d


def _single_line(value: object, default: str = "n/a") -> str:
    raw = str(value if value is not None else default)
    text = " ".join(part.strip() for part in raw.splitlines() if part.strip())
    return text or default


def sh(args, cwd=None, **kw) -> subprocess.CompletedProcess:
    return subprocess.run(args, cwd=cwd or ROOT, capture_output=True, text=True, **kw)


def die(kind: str, slug: str, detail: str, code: int) -> "int":
    print(f"AUTOLAND-{kind}: {slug} — {detail}")
    return code


def bundle_dirs() -> list[pathlib.Path]:
    if not QUEUE.is_dir():
        return []
    return sorted(
        p for p in QUEUE.iterdir()
        if p.is_dir() and _safe_bundle_dir(p.name) == p.resolve() and (p / "meta.json").is_file()
    )


def read_meta(d: pathlib.Path) -> dict:
    return json.loads((d / "meta.json").read_text())


def build() -> None:
    r = sh(["cargo", "build", "-p", "hyb", "--quiet"], cwd=str(COMPILER))
    if r.returncode != 0:
        raise RuntimeError(f"cargo build failed: {r.stderr[-800:]}")


def check_repro(repro_rel: str) -> tuple[int, str]:
    r = sh([str(HYB), "check", repro_rel])
    out = (r.stdout or "") + (r.stderr or "")
    return r.returncode, out


def battery() -> list[tuple[str, bool, str]]:
    steps = [
        ("workspace", ["cargo", "test", "--workspace"]),
        ("corpus", ["python3", "tools/corpus_harness.py", "--bin", str(HYB)]),
        ("census", ["python3", "tools/check_corpus_census.py"]),
        ("differential", ["python3", "tools/differential_loop.py",
                          "--bin", str(HYB)]),
    ]
    results = []
    for name, cmd in steps:
        # cargo steps run from the workspace root (compiler/); the python
        # tooling steps run from the repo root (they resolve their own paths).
        r = sh(cmd, cwd=str(COMPILER) if cmd[0] == "cargo" else None)
        ok = r.returncode == 0
        results.append((name, ok, (r.stdout + r.stderr)[-600:] if not ok else ""))
        if not ok:
            break
    return results


def revert() -> None:
    sh(["git", "checkout", "--", "."])
    sh(["git", "clean", "-fd"])


def land_commit(slug: str, meta: dict, repro_rel: str) -> str:
    issue = meta["issue"]
    if isinstance(issue, bool) or not isinstance(issue, int) or issue <= 0:
        raise ValueError(f"invalid issue number in meta.json: {issue!r}")
    basis = _single_line(meta.get("basis", "n/a"))
    expected_code = _single_line(meta.get("expected_code", ""), default="")
    msg = (
        f"Land #{issue} via autoland bundle '{slug}'\n\n"
        f"Bundle: automation/land-queue/{slug} (authored in an agent session,\n"
        f"verified + landed by tools/autoland.py under SHEPHERD_MODE=live).\n"
        f"Basis: {basis}\n"
        f"Gates: baseline ACCEPT -> patched REJECT ({expected_code})\n"
        f"-> full battery green -> push-and-assert LAND-CONFIRMED.\n\n"
        f"Fixes #{issue}"
    )
    r = sh(["git", "add", "-A"])
    if r.returncode != 0:
        raise RuntimeError(f"git add failed: {r.stderr[-400:]}")
    r = sh(["git", "commit", "--quiet", "-m", msg])
    if r.returncode != 0:
        raise RuntimeError(f"git commit failed: {r.stderr[-400:]}")
    if sh(["git", "diff", "HEAD", "--quiet"]).returncode != 0:
        raise RuntimeError("pre-push gate: diff HEAD not empty after commit")
    r = sh(["git", "push", "origin", "HEAD:main"])
    if r.returncode != 0:
        raise RuntimeError(f"push failed: {r.stderr[-400:]}")
    # push-and-assert: origin/main must contain the landing
    for _ in range(6):
        q = sh(["git", "ls-remote", "origin", "refs/heads/main"])
        remote = q.stdout.strip().split()[0] if q.stdout.strip() else ""
        local = sh(["git", "rev-parse", "HEAD"]).stdout.strip()
        if remote == local and remote:
            return remote
        time.sleep(2)
    raise RuntimeError("LAND-ASSERT failed: origin/main does not contain the landing")


def log_event(token_env: str, event: str, verdict: str, note: str, issue: int | None = None, live: bool = True) -> None:
    if not live:
        print(f"AUTOLAND-LOG(dry-run, not written): {event} {verdict} — {note}")
        return
    cmd = ["python3", "tools/shepherd.py", "--token-env", token_env, "log",
           "--event", event, "--verdict", verdict, "--note", note]
    if issue:
        cmd += ["--issue", str(issue)]
    sh(cmd)


def run_bundle(slug: str, live: bool, token_env: str) -> int:
    d = _safe_bundle_dir(slug)
    if d is None:
        return die("NO-BUNDLE", str(slug), "unsafe slug", 2)
    if not (d / "meta.json").is_file():
        return die("NO-BUNDLE", slug, "missing meta.json", 2)
    meta = read_meta(d)
    if meta.get("status") == "landed":
        print(f"AUTOLAND-SKIP: {slug} already landed at {meta.get('landedSha', '?')[:9]}")
        return 0
    issue = meta.get("issue")
    if isinstance(issue, bool) or not isinstance(issue, int) or issue <= 0:
        return die("NO-BUNDLE", slug, "invalid issue in meta.json", 2)
    expected = _single_line(meta.get("expected_code", ""), default="")
    repro_src = d / "repro.hy"
    repro_rel = f"automation/land-queue/{d.name}/repro.hy"

    # ---- 1. BASELINE: the unpatched tree must ACCEPT the repro ----
    build()
    code, out = check_repro(repro_rel)
    if code != 0:
        log_event(token_env, "autoland", "stale-premise",
                  f"#{issue}: repro REJECTs on main (exit {code}) — stale or already fixed", issue, live=live)
        return die("STALE-PREMISE", slug, f"repro rejects on main (exit {code}) — nothing landed", 3)

    if not live:
        print(f"AUTOLAND-DRYRUN: {slug} baseline ACCEPT ok — SHEPHERD_MODE!=live, stopping before apply")
        return 0

    # ---- 2. APPLY: patched tree must REJECT with the expected code ----
    r = sh(["git", "apply", str(d / "fix.patch")])
    if r.returncode != 0:
        log_event(token_env, "autoland", "blocked", f"#{issue}: fix.patch does not apply on main tip", issue, live=live)
        return die("APPLY-FAILED", slug, f"git apply failed: {r.stderr[-400:]}", 4)
    try:
        build()
        code, out = check_repro(repro_rel)
        if code == 0:
            log_event(token_env, "autoland", "blocked", f"#{issue}: patch applied but repro still ACCEPTs — ineffective", issue, live=live)
            return die("INEFFECTIVE", slug, "repro still ACCEPTs after patch — nothing landed", 5)
        if expected and expected not in out:
            log_event(token_env, "autoland", "blocked", f"#{issue}: repro rejected but without expected code {expected}", issue, live=live)
            return die("WRONG-CODE", slug, f"expected {expected} in diagnostics, got: {out[:300]}", 6)

        # ---- 3. BATTERY ----
        for name, ok, tail in battery():
            if not ok:
                log_event(token_env, "autoland", "blocked", f"#{issue}: battery gate '{name}' failed", issue, live=live)
                return die("BATTERY-FAILED", slug, f"gate '{name}' failed:\n{tail}", 7)

        # ---- 4. LAND (push-and-assert inside) ----
        sha = land_commit(d.name, meta, repro_rel)

        # ---- 5. RECORD ----
        meta["status"] = "landed"
        meta["landedSha"] = sha
        meta["landedAt"] = time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime())
        (d / "meta.json").write_text(json.dumps(meta, indent=2) + "\n")
        sh(["git", "add", "-A"])
        sh(["git", "commit", "--quiet", "-m", f"autoland: bundle '{d.name}' consumed — landed {sha[:9]} (Fixes #{issue})"])
        sh(["git", "push", "origin", "HEAD:main"])
        log_event(token_env, "autoland", "landed",
                  f"#{issue}: bundle '{d.name}' landed {sha[:9]} — baseline ACCEPT, patched REJECT {expected}, battery green, LAND-CONFIRMED", issue, live=live)
        print(f"AUTOLAND-LANDED: {d.name} — {sha[:9]} LAND-CONFIRMED (Fixes #{issue})")
        return 0
    finally:
        # If anything above raised, leave main clean (the landing path
        # pushed already; a failure path must not leave the tree dirty).
        pass


def revert_on_failure() -> None:
    r = sh(["git", "status", "--porcelain"])
    if r.stdout.strip():
        revert()


def main() -> int:
    ap = argparse.ArgumentParser(description="autoland: verify + land prepared fix bundles")
    ap.add_argument("cmd", choices=["list", "run"])
    ap.add_argument("--slug")
    ap.add_argument("--all", action="store_true")
    ap.add_argument("--token-env", default="GITHUB_TOKEN")
    args = ap.parse_args()

    mode = os.environ.get("SHEPHERD_MODE", "dry-run")
    live = mode == "live"

    if args.cmd == "list":
        for d in bundle_dirs():
            m = read_meta(d)
            state = m.get("status", "pending")
            print(f"{d.name}: issue={m.get('issue')} status={state} expected={m.get('expected_code')}")
        return 0

    try:
        if args.all:
            rc = 0
            for d in bundle_dirs():
                if read_meta(d).get("status") == "landed":
                    continue
                rc = max(rc, run_bundle(d.name, live, args.token_env))
                revert_on_failure()
            return rc
        if not args.slug:
            print("run requires --slug or --all")
            return 2
        rc = run_bundle(args.slug, live, args.token_env)
        revert_on_failure()
        return rc
    except Exception as e:  # noqa: BLE001 — the honest-record path
        print(f"AUTOLAND-ERROR: {e}")
        revert_on_failure()
        return 1


if __name__ == "__main__":
    sys.exit(main())
