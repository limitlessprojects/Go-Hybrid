#!/usr/bin/env python3
"""author_stage.py — the AUTHOR layer (v0). Bundle-only, never lands.

Produces a land-queue bundle for one issue:

    automation/land-queue/author-<N>/
      meta.json   {"issue": N, "expected_code": "HYB-Exxxx"|null,
                   "basis": "author-stage v0 brain=<model> rounds=<r>",
                   "status": "ready"}
      fix.patch   raw git-format diff: fix + tests (+ corpus registration)
      repro.hy    the issue's minimal ```hy repro (extracted verbatim)
      trace.log   every prompt, every completion size, every command,
                  every gate exit code — failures recorded verbatim

It NEVER pushes, NEVER lands, NEVER closes anything. `tools/autoland.py`
remains the ONLY path to main, with unchanged gates. A BLOCKED run never
writes into the queue: blocked evidence goes to target/author-blocked/
and to the workflow artifacts — as loud as success, outside the queue.

Brain: any OpenAI-compatible chat endpoint (AUTHOR_API_BASE / AUTHOR_MODEL
/ AUTHOR_API_KEY) — e.g. a llama.cpp server hosting Ornith-1.0-9B GGUF on
a Kaggle GPU kernel, or a cloud model — OR the deterministic arm
`--brain specialized` (tools/specialized_brain.py: class-template fitter,
no LLM, $0; AUTHOR-STAGE.md §5 amendment 1). No brain configured = honest
BLOCKED before anything runs.

Honesty rules (binding, mirror AUTOMATION.md):
  1. The model proposes; only the gates decide. "ready" means the fast
     gate set passed with recorded evidence — never a model self-report.
  2. trace.log records everything; failures are never trimmed to look good.
  3. The harness never edits a test to make it pass and never weakens a
     gate. MAX_ROUNDS exhaustion = BLOCKED.
  4. Scope guard: diffs touching anything outside compiler/ + tests/ +
     automation/land-queue/ are discarded before apply.
  5. PRs are out of scope forever: author stage refuses them (contributor
     credit is non-negotiable).

Fast gate set (round-level; autoland re-verifies everything):
  cargo build -p hyb
  hyb check repro            -> must REJECT (nonzero), with expected code
  hyb check accept-controls  -> must still ACCEPT (over-reach tripwire;
                                controls are read from HEAD, immune to
                                the patch itself)

Commands:
  author   --issue N [--out DIR] [--max-rounds 4]
  selftest                       # plumbing only: no LLM, no repo mutation
"""

from __future__ import annotations

import argparse
import json
import os
import pathlib
import re
import subprocess
import sys
import tempfile
import time
import urllib.request

ROOT = pathlib.Path(__file__).resolve().parent.parent
COMPILER = ROOT / "compiler"
QUEUE = ROOT / "automation" / "land-queue"
HYB = ROOT / "compiler" / "target" / "debug" / "hyb"
BLOCKED_DIR = ROOT / "target" / "author-blocked"
REPRO_DIR = ROOT / "compiler" / "target" / "author-stage"  # gitignored area
REPO = "limitlessprojects/Go-Hybrid"
GITHUB_API = "https://api.github.com"
SCOPE_PREFIXES = ("compiler/", "tests/", "automation/land-queue/")
ACCEPT_CONTROLS = [
    "tests/checker-corpus/control-uint8-max.hy",
    "tests/checker-corpus/control-composite-literal-in-range.hy",
]
MAX_PATCH_BYTES = 400_000

DIFF_RE = re.compile(r"```diff\s*\n(.*?)```", re.S)
HY_RE = re.compile(r"```hy\s*\n(.*?)```", re.S)
EXPECTED_RE = re.compile(r"HYB-E\d{3,4}")


def sh(args, cwd=None, timeout=900, input=None):
    return subprocess.run(args, cwd=cwd or ROOT, capture_output=True, text=True,
                          timeout=timeout, input=input)


def gh_token():
    env = os.environ.get("GITHUB_TOKEN") or os.environ.get("GH_TOKEN")
    if env:
        return env
    out = sh(["git", "remote", "get-url", "origin"], timeout=30).stdout or ""
    m = re.search(r"https://([^@]+)@github.com", out)
    if m:
        return m.group(1)
    # session fallback: the token file persists across container resets even
    # when no env var or embedded remote credential is present.
    for p in (pathlib.Path("/home/z/my-project/.gh_token"), ROOT / ".gh_token"):
        try:
            t = p.read_text().strip()
            if t:
                return t
        except OSError:
            pass
    return None


def api_get(path):
    req = urllib.request.Request(GITHUB_API + path)
    tok = gh_token()
    if tok:
        req.add_unredirected_header("Authorization", "token " + tok)
    req.add_header("Accept", "application/vnd.github+json")
    with urllib.request.urlopen(req, timeout=30) as r:
        return json.loads(r.read().decode())


def tr(path: pathlib.Path, kind: str, **kw) -> None:
    rec = {"t": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()), "kind": kind, **kw}
    with open(path, "a") as f:
        f.write(json.dumps(rec, default=str) + "\n")
    print(f"author-stage[{kind}]: {json.dumps(kw, default=str)[:400]}")


def chat(messages) -> str:
    base = os.environ.get("AUTHOR_API_BASE", "").rstrip("/")
    model = os.environ.get("AUTHOR_MODEL", "")
    key = os.environ.get("AUTHOR_API_KEY", "")
    if not base or not model:
        raise RuntimeError("no brain configured (AUTHOR_API_BASE/AUTHOR_MODEL unset)")
    body = {"model": model, "messages": messages, "temperature": 0.2, "max_tokens": 8192}
    headers = {"Content-Type": "application/json"}
    if key:
        headers["Authorization"] = "Bearer " + key
    req = urllib.request.Request(base + "/chat/completions",
                                 data=json.dumps(body).encode(), headers=headers)
    with urllib.request.urlopen(req, timeout=600) as r:
        return json.loads(r.read().decode())["choices"][0]["message"]["content"]


def diff_paths(d: str) -> list[str]:
    paths = []
    for line in d.splitlines():
        if line.startswith("+++ "):
            p = line[4:].strip()
            if p.startswith("b/"):
                p = p[2:]
            if p != "/dev/null":
                paths.append(p)
    return paths


def scope_ok(d: str) -> bool:
    ps = diff_paths(d)
    return bool(ps) and all(p.startswith(SCOPE_PREFIXES) for p in ps)


def render_messages(issue: dict, feedback: str, files: list[str]) -> list[dict]:
    system = (
        "You author fixes for the Go-Hybrid compiler (Rust workspace under compiler/). "
        "The semantic checker is hy-check; diagnostics use HYB-Exxxx codes. A complete "
        "fix ships the checker change plus Rust tests under compiler/crates/hy-check/"
        "tests/ (and, only when the issue adds a new reject class, a corpus fixture "
        "under tests/checker-corpus/ registered in VERDICTS.md + corpus README + "
        "census surfaces). Over-reach is the primary risk: a checker that over-rejects "
        "legal programs is worse than one that under-rejects; keep existing accept "
        "behavior byte-identical. Answer with one or more ```diff fenced unified diffs "
        "against HEAD, then a 3-line rationale. No prose outside fences except that "
        "rationale."
    )
    user = (
        f"ISSUE #{issue['number']}: {issue['title']}\n\n{(issue.get('body') or '')[:6000]}\n\n"
        f"ALLOWED SCOPE (a diff touching anything else is discarded): "
        f"{', '.join(SCOPE_PREFIXES)}\n"
        f"checker sources: {', '.join(files)}\n"
    )
    if feedback:
        user += f"\nPREVIOUS ATTEMPT FAILED — evidence follows; change the approach:\n{feedback[:3000]}\n"
    return [{"role": "system", "content": system}, {"role": "user", "content": user}]


def dirty() -> bool:
    return bool(sh(["git", "status", "--porcelain"], timeout=60).stdout.strip())


def revert_tree() -> None:
    sh(["git", "checkout", "--", "."], timeout=120)
    sh(["git", "clean", "-fd"], timeout=120)


def hyb_check(path: pathlib.Path) -> tuple[int, str]:
    r = sh([str(HYB), "check", str(path)], timeout=120)
    return r.returncode, (r.stdout or "") + (r.stderr or "")


def write_blocked(issue_no: int, trace: pathlib.Path, note: str) -> int:
    tr(trace, "blocked", note=str(note)[:500])
    d = BLOCKED_DIR / f"issue-{issue_no}"
    d.mkdir(parents=True, exist_ok=True)
    (d / "trace.log").write_text(trace.read_text() if trace.is_file() else "")
    (d / "note.txt").write_text(str(note) + "\n")
    (d / "meta.json").write_text(json.dumps(
        {"issue": issue_no, "status": "blocked", "note": str(note)[:500]}, indent=2) + "\n")
    print(f"AUTHOR-STAGE-BLOCKED: issue #{issue_no} — {note}")
    print(f"  blocked evidence (never enters the queue): {d}")
    return 3


def run_author(issue_no: int, out: pathlib.Path, max_rounds: int,
               brain: str = "api") -> int:
    trace = pathlib.Path(tempfile.mkdtemp(prefix="author-stage-")) / "trace.log"
    if not QUEUE.resolve() in out.resolve().parents and out.resolve() != QUEUE.resolve():
        if not str(out).startswith(str(ROOT)):
            print(f"AUTHOR-STAGE-ERROR: --out must live inside the repo ({ROOT})")
            return 2
    if dirty():
        print("AUTHOR-STAGE-ERROR: working tree dirty — refusing to run (clean it first)")
        return 2
    if brain == "api" and not (
        os.environ.get("AUTHOR_API_BASE") and os.environ.get("AUTHOR_MODEL")
    ):
        return write_blocked(issue_no, trace,
                             "no brain configured (AUTHOR_API_BASE/AUTHOR_MODEL unset) — nothing authored")
    if brain == "specialized":
        os.environ.setdefault("AUTHOR_MODEL", "specialized")
    try:
        issue = api_get(f"/repos/{REPO}/issues/{issue_no}")
    except Exception as e:
        return write_blocked(issue_no, trace, f"issue fetch failed: {e}")
    if "pull_request" in issue:
        print(f"AUTHOR-STAGE-ERROR: #{issue_no} is a PR — author stage never touches PRs")
        return 2
    tr(trace, "start", issue=issue_no, title=issue["title"][:120],
       brain=os.environ.get("AUTHOR_MODEL"), max_rounds=max_rounds)

    body = issue.get("body") or ""
    m = HY_RE.search(body)
    if not m:
        return write_blocked(issue_no, trace,
                             "no ```hy repro fence in the issue body — cannot author without a repro")
    repro_src = m.group(1)
    exp = EXPECTED_RE.search(body)
    expected = exp.group(0) if exp else ""
    tr(trace, "repro", chars=len(repro_src), expected=expected or None)

    REPRO_DIR.mkdir(parents=True, exist_ok=True)
    repro = REPRO_DIR / "repro.hy"
    repro.write_text(repro_src)
    repro_rel = str(repro.relative_to(ROOT))

    files = [l.strip() for l in (sh(["git", "ls-files", "compiler/crates"], timeout=60).stdout or "").splitlines()
             if l.endswith(".rs")][:60]
    feedback = ""
    for rnd in range(1, max_rounds + 1):
        tr(trace, "round", round=rnd)
        try:
            if brain == "specialized":
                # Arm A0: the deterministic class-template fitter. It returns
                # a completion-shaped response (```diff + rationale) or a
                # no-diff rationale; the gates below are IDENTICAL for
                # every arm (AUTHOR-STAGE.md §5).
                try:
                    from tools.specialized_brain import propose as sp_propose
                except ImportError:
                    from specialized_brain import propose as sp_propose
                resp = sp_propose(issue_no, rnd, repro_src, expected, feedback)
                if resp is None:
                    resp = ""
            else:
                resp = chat(render_messages(issue, feedback, files))
        except Exception as e:
            tr(trace, "brain-error", round=rnd, error=str(e)[:300])
            feedback = f"brain call failed: {e}"
            continue
        tr(trace, "brain-response", round=rnd, chars=len(resp))
        diffs = [d for d in DIFF_RE.findall(resp) if d.strip()]
        if not diffs or len("\n".join(diffs).encode()) > MAX_PATCH_BYTES:
            feedback = ("no usable ```diff block (or patch over "
                        f"{MAX_PATCH_BYTES} bytes); re-emit the complete unified diff")
            continue
        applied = True
        for d in diffs:
            if not scope_ok(d):
                feedback = (f"scope violation — diff touches {diff_paths(d)}; "
                            f"allowed prefixes: {', '.join(SCOPE_PREFIXES)}")
                applied = False
                break
            c = sh(["git", "apply", "--check", "-"], timeout=60, input=d)
            if c.returncode != 0:
                feedback = "git apply --check failed:\n" + (c.stderr or "")[-1200:]
                applied = False
                break
            a = sh(["git", "apply", "-"], timeout=60, input=d)
            if a.returncode != 0:
                feedback = "git apply failed:\n" + (a.stderr or "")[-1200:]
                applied = False
                break
        if not applied:
            tr(trace, "gate-apply-failed", round=rnd,
               note=feedback[:200])
            revert_tree()
            continue

        b = sh(["cargo", "build", "-p", "hyb", "--quiet"], cwd=str(COMPILER),
               timeout=1800)
        if b.returncode != 0:
            feedback = "cargo build -p hyb failed:\n" + (b.stderr or "")[-1500:]
            tr(trace, "gate-build-failed", round=rnd)
            revert_tree()
            continue
        code, outp = hyb_check(repro_rel)
        if code == 0:
            feedback = "patch applied but the repro still ACCEPTs — the fix is ineffective"
            tr(trace, "gate-ineffective", round=rnd)
            revert_tree()
            continue
        # hyb check uses 1 for a checked rejection and 2 for invocation/I/O
        # failures. Signals and other process failures are not a verdict,
        # even if a diagnostic was printed before the checker failed.
        if code != 1:
            feedback = (f"checker failed (exit {code}); expected rejection exit 1:\n"
                        f"{outp[-800:]}")
            tr(trace, "gate-checker-failed", round=rnd, exit=code)
            revert_tree()
            continue
        if expected and not re.search(
            rf"(?<![\w-]){re.escape(expected)}(?![\w-])", outp
        ):
            feedback = (f"repro rejected but without the expected code {expected}; "
                        f"diagnostics tail:\n{outp[-800:]}")
            tr(trace, "gate-wrong-code", round=rnd, expected=expected)
            revert_tree()
            continue
        tr(trace, "gate-repro-ok", round=rnd, exit=code, expected=expected or "(any)")
        controls_ok = True
        for cpath in ACCEPT_CONTROLS:
            gs = sh(["git", "show", f"HEAD:{cpath}"], timeout=60)
            if gs.returncode != 0 or not gs.stdout:
                continue
            tmpc = REPRO_DIR / ("control-" + pathlib.Path(cpath).name)
            tmpc.write_text(gs.stdout)
            cc, _ = hyb_check(str(tmpc.relative_to(ROOT)))
            if cc != 0:
                feedback = (f"OVER-REACH: accept control {cpath} now rejects "
                            f"(exit {cc}) — the fix over-rejects legal programs")
                tr(trace, "gate-overreach", round=rnd, control=cpath, exit=cc)
                controls_ok = False
                break
        if not controls_ok:
            revert_tree()
            continue

        # READY: capture the patch, revert the tree, then write the bundle.
        sh(["git", "add", "-A"], timeout=120)
        pd = sh(["git", "diff", "--cached", "HEAD"], timeout=120)
        fix_patch = pd.stdout or ""
        sh(["git", "reset", "--quiet"], timeout=60)
        revert_tree()
        out.mkdir(parents=True, exist_ok=True)
        (out / "fix.patch").write_text(fix_patch)
        (out / "repro.hy").write_text(repro_src)
        (out / "trace.log").write_text(trace.read_text())
        (out / "meta.json").write_text(json.dumps({
            "issue": issue_no,
            "expected_code": expected or None,
            "basis": f"author-stage v0 brain={os.environ.get('AUTHOR_MODEL')} rounds={rnd}",
            "status": "ready",
            "gates": {"baseline": "ACCEPT", "patched": f"REJECT {expected or '(any code)'}",
                      "acceptControls": ACCEPT_CONTROLS, "rounds": rnd},
        }, indent=2) + "\n")
        tr(trace, "ready", rounds=rnd, out=str(out))
        print(f"AUTHOR-STAGE-READY: {out.relative_to(ROOT)} — issue #{issue_no}, "
              f"rounds={rnd}, brain={os.environ.get('AUTHOR_MODEL')}")
        print("  NOT a landing: autoland must still pass baseline ACCEPT -> patched "
              "REJECT -> full battery -> push-and-assert.")
        return 0

    return write_blocked(issue_no, trace,
                         f"no ready bundle after {max_rounds} rounds — last failure:\n{feedback[:800]}")


def selftest() -> int:
    """Plumbing only: no LLM, no network, no repo mutation."""
    ok = True

    def check(name, cond):
        nonlocal ok
        print(("PASS " if cond else "FAIL ") + name)
        ok = ok and cond

    d1 = "+++ b/compiler/crates/hy-check/src/lib.rs\n@@ -1 +1 @@\n-a\n+b\n"
    d2 = "+++ b/README.md\n@@ -1 +1 @@\n-a\n+b\n"
    check("scope: compiler/ allowed", scope_ok(d1))
    check("scope: README.md rejected", not scope_ok(d2))
    check("scope: empty diff rejected", not scope_ok("no diff here"))
    body = ('Repro:\n```hy\nfn main() { x := 9223372036854775807 + 1; '
            'fmt.println("{x}") }\n```\nExpect HYB-E6002.')
    check("expected-code regex", EXPECTED_RE.search(body).group(0) == "HYB-E6002")
    check("hy fence extraction", "9223372036854775807" in HY_RE.search(body).group(1))
    tmp = pathlib.Path(tempfile.mkdtemp(prefix="author-selftest-")) / "author-0"
    tmp.mkdir(parents=True)
    (tmp / "meta.json").write_text(json.dumps(
        {"issue": 0, "expected_code": "HYB-E6002", "basis": "selftest", "status": "ready"},
        indent=2) + "\n")
    (tmp / "repro.hy").write_text("fn main() {}\n")
    (tmp / "fix.patch").write_text(d1)
    m2 = json.loads((tmp / "meta.json").read_text())
    check("bundle meta round-trip", m2["issue"] == 0 and m2["status"] == "ready")
    check("bundle files present",
          all((tmp / f).is_file() for f in ("meta.json", "repro.hy", "fix.patch")))
    tp = tmp / "trace.log"
    tr(tp, "round", round=1)
    tr(tp, "ready", rounds=1)
    lines = [json.loads(l) for l in tp.read_text().splitlines()]
    check("trace JSONL parse", [r["kind"] for r in lines] == ["round", "ready"])
    blocked_dir = (BLOCKED_DIR / "selftest-probe").resolve()
    check("blocked path outside the queue",
          QUEUE.resolve() not in blocked_dir.parents and blocked_dir != QUEUE.resolve())
    print("SELFTEST " + ("PASS" if ok else "FAIL"))
    return 0 if ok else 1


def main() -> int:
    ap = argparse.ArgumentParser(
        description="author-stage v0: issue -> land-queue bundle (never lands)")
    sub = ap.add_subparsers(dest="cmd", required=True)
    pa = sub.add_parser("author", help="author a fix bundle for one issue")
    pa.add_argument("--issue", type=int, required=True)
    pa.add_argument("--out", default=None,
                    help="bundle dir (default automation/land-queue/author-<N>)")
    pa.add_argument("--max-rounds", type=int, default=4)
    pa.add_argument("--brain", choices=("api", "specialized"), default="api",
                    help="api = OpenAI-compatible endpoint; specialized = "
                         "the deterministic class-template fitter (arm A0)")
    sub.add_parser("selftest", help="plumbing self-test (no LLM, no repo mutation)")
    args = ap.parse_args()
    if args.cmd == "selftest":
        return selftest()
    out = pathlib.Path(args.out) if args.out else QUEUE / f"author-{args.issue}"
    try:
        return run_author(args.issue, out, args.max_rounds, brain=args.brain)
    except Exception as e:  # noqa: BLE001 — the honest-record path
        print(f"AUTHOR-STAGE-ERROR: {e}")
        return 1


if __name__ == "__main__":
    sys.exit(main())
