#!/usr/bin/env python3
"""specialized_brain.py — bake-off arm A0 (AUTHOR-STAGE.md §5 amendment 1).

A deterministic per-class template fitter: NO LLM, NO network, $0 compute.
It emits the same artifact any other brain would — a completion containing
```diff blocks that tools/author_stage.py's UNCHANGED gate loop applies and
re-verifies (build, repro REJECT + expected code, accept controls read from
HEAD).

Template families (CLASS-LEVEL transplants of landed fix patterns ONLY —
the anti-Goodhart binding): sp_tmpl_e6001 (#758), sp_tmpl_e6006 (#823),
sp_tmpl_e2009 (#827). An issue outside the families = BLOCK, never
hand-tailored.

Internal pre-verification (disclosed in every rationale, deterministic):
apply candidate → cargo build -p hyb → repro REJECT + expected code →
accept controls ACCEPT → emitted Rust tests pass → corpus harness → census
check → only then propose. The harness gates re-verify independently.

Descent: candidates per family in declared order (full → narrower); the
first internally-green candidate is round-1's proposal. If none is green,
propose returns a no-diff rationale (the loop BLOCKs after its own rounds)
and writes the full internal evidence to target/author-brain-trace/.
"""

from __future__ import annotations

import difflib
import os
import pathlib
import re
import subprocess
import time

# so `import sp_tmpl_e6001` works when the repo is on sys.path as tools/
import sys

sys.path.insert(0, str(pathlib.Path(__file__).resolve().parent))

# The harness pins HYB at compiler/target/debug/hyb — keep cargo's default
# target dir so the smoke and the harness gates see the SAME binary.
os.environ.pop("CARGO_TARGET_DIR", None)

ROOT = pathlib.Path(__file__).resolve().parent.parent
COMPILER = ROOT / "compiler"
HYB = COMPILER / "target" / "debug" / "hyb"
SMOKE_DIR = COMPILER / "target" / "author-stage"
BRAIN_TRACE = ROOT / "target" / "author-brain-trace"

ACCEPT_CONTROLS = [
    "tests/checker-corpus/control-uint8-max.hy",
    "tests/checker-corpus/control-composite-literal-in-range.hy",
]

FAMILIES = {
    "HYB-E6001": "sp_tmpl_e6001",
    "HYB-E6006": "sp_tmpl_e6006",
    "HYB-E2009": "sp_tmpl_e2009",
    "HYB-E2002": "sp_tmpl_ebind",
    "HYB-E1002": "sp_tmpl_ebind",
}

# Issues whose body names no HYB-E code route by class membership (the
# issue names the rule family's file/rule; the anti-Goodhart binding is
# unchanged: the class template must still pass every gate, else BLOCK).
ISSUE_FAMILY_HINTS = {
    905: "sp_tmpl_ebind",
}


def sh(args, cwd=None, timeout=2400, input=None):
    return subprocess.run(args, cwd=cwd or str(ROOT), capture_output=True,
                          text=True, timeout=timeout, input=input)


def head(path: str) -> str:
    r = sh(["git", "show", f"HEAD:{path}"], timeout=60)
    if r.returncode != 0:
        raise RuntimeError(f"git show HEAD:{path} failed: {r.stderr[:200]}")
    return r.stdout


def apply_candidate(cand: dict) -> None:
    for path, old, new, *rest in cand["edits"]:
        p = ROOT / path
        src = p.read_text()
        if rest and rest[0]:  # replace_all
            if src.count(old) < 1:
                raise RuntimeError(f"anchor missing in {path}: {old[:90]!r}")
            p.write_text(src.replace(old, new))
        else:
            n = src.count(old)
            if n != 1:
                raise RuntimeError(f"anchor occurs {n}x in {path}: {old[:90]!r}")
            p.write_text(src.replace(old, new))
    for path, content in cand["new_files"]:
        (ROOT / path).write_text(content)


def revert_tree() -> None:
    sh(["git", "checkout", "--", "."], timeout=180)
    sh(["git", "clean", "-fd"], timeout=180)


def _bump(m: re.Match) -> str:
    total, acc, managed, rej = (int(m.group(i)) for i in range(1, 5))
    return (f"**Current registered corpus: {total + 1} programs — "
            f"ACCEPT {acc} · ACCEPT-MANAGED {managed} · REJECT {rej + 1}**")


def surface_edits(issue_no: int, slug: str, why: str) -> list[tuple]:
    """Census surfaces — ALL 8, each set to the NEW corpus total (total+1).

    The census tool (tools/check_corpus_census.py) compares every claim
    against the corpus directory. Note: ci.yml + Makefile were left at 120
    by the #998/#999 landing while the corpus moved to 122 — the census was
    already drifting at HEAD; this bundle repairs that as a side effect.
    """
    edits = []

    rm = head("tests/checker-corpus/README.md")
    m = re.search(r"\*\*Current registered corpus: (\d+) programs — ACCEPT (\d+)"
                  r" · ACCEPT-MANAGED (\d+) · REJECT (\d+)", rm)
    if not m:
        raise RuntimeError("corpus README count line not found")
    total, acc, managed, rej = (int(m.group(i)) for i in range(1, 5))
    t1, r1 = total + 1, rej + 1
    edits.append(("tests/checker-corpus/README.md", m.group(0),
                  f"**Current registered corpus: {t1} programs — "
                  f"ACCEPT {acc} · ACCEPT-MANAGED {managed} · REJECT {r1}",
                  True))  # the count line appears twice (lede + body)

    def bump(path, pattern, flags=0):
        src = head(path)
        mm = re.search(pattern, src, flags)
        if not mm:
            raise RuntimeError(f"census surface not found in {path}: {pattern}")
        old = mm.group(0)
        # Dual-number surfaces (`(\d+)/\d+` in ci.yml/Makefile/README.md and
        # `files.len(), (\d+), "expected exactly \d+ corpus programs"` in
        # parse_all.rs) state the corpus total twice inside `old`. Replacing
        # every digit run in the matched span updates both numbers in lockstep
        # and repairs any previously drifted denominator.
        edits.append((path, old, re.sub(r"\d+", str(t1), old)))

    bump("compiler/crates/hy-syntax/tests/parse_all.rs",
         r"files\.len\(\),\s*(\d+),\s*\"expected exactly \d+ corpus programs\"")
    bump(".github/workflows/ci.yml", r"verdict agreement \((\d+)/\d+ \+ tripwire\)")
    bump("Makefile", r"corpus verdicts \((\d+)/\d+\)")
    bump("README.md", r"checker corpus (\d+)/\d+")
    bump("GUARANTEES.md", r"^\s*(\d+) programs, each with an", re.M)
    bump("compiler/crates/hy-syntax/src/ast.rs", r"checker-corpus/\*\.hy \((\d+) programs\)")

    vm = head("tests/checker-corpus/VERDICTS.md")
    section = (
        f"\n-------------------------------------------------------------------------------\n\n"
        f"### {slug} (issue #{issue_no}) — REJECT\n\n{why}\n\n"
        f"With this registration the corpus is **{t1} programs: ACCEPT {acc} ·\n"
        f"ACCEPT-MANAGED {managed} · REJECT {r1}** — main's {total} plus this one.\n")
    tail_pat = r"With this registration the corpus is \*\*\d+ programs: ACCEPT \d+ ·\s*\nACCEPT-MANAGED \d+ · REJECT \d+\*\*[^\n]*\n?"
    tails = re.findall(tail_pat, vm)
    if tails:
        tail = tails[-1]  # the CURRENT registration sentence, not a historical one
        edits.append(("tests/checker-corpus/VERDICTS.md", tail, tail + section))
    else:
        edits.append(("tests/checker-corpus/VERDICTS.md", vm[-200:], vm[-200:] + section))
    return edits


def smoke(repro_rel: str, expected: str, rust_tests: list[str]) -> tuple[bool, str]:
    b = sh(["cargo", "build", "-p", "hyb", "--quiet"], cwd=str(COMPILER), timeout=3000)
    if b.returncode != 0:
        return False, "cargo build -p hyb failed:\n" + (b.stderr or "")[-1500:]
    r = sh([str(HYB), "check", repro_rel], timeout=300)
    outp = (r.stdout or "") + (r.stderr or "")
    if r.returncode == 0:
        return False, "repro still ACCEPTs — candidate ineffective"
    if r.returncode != 1:
        return False, (f"checker failed (exit {r.returncode}); expected rejection exit 1:\n"
                       + outp[-600:])
    if expected and not re.search(
        rf"(?<![\w-]){re.escape(expected)}(?![\w-])", outp
    ):
        return False, f"repro rejected without {expected}; tail:\n" + outp[-600:]
    SMOKE_DIR.mkdir(parents=True, exist_ok=True)
    for c in ACCEPT_CONTROLS:
        tmp = SMOKE_DIR / ("ctl-" + pathlib.Path(c).name)
        gs = sh(["git", "show", f"HEAD:{c}"], timeout=60)
        tmp.write_text(gs.stdout)
        cc = sh([str(HYB), "check", str(tmp.relative_to(ROOT))], timeout=300)
        if cc.returncode != 0:
            return False, f"OVER-REACH: accept control {c} now rejects (exit {cc.returncode})"
    if rust_tests:
        t = sh(["cargo", "test", "-p", "hy-check"] + rust_tests + ["--quiet"],
               cwd=str(COMPILER), timeout=3000)
        if t.returncode != 0:
            return False, "emitted Rust tests failed:\n" + (t.stdout or "")[-1500:]
    c2 = sh(["python3", "tools/corpus_harness.py", "--bin", str(HYB)], timeout=1800)
    if c2.returncode != 0:
        ev = ((c2.stdout or "") + (c2.stderr or ""))[-1200:]
        return False, "corpus harness failed (exit %d):\n%s" % (c2.returncode, ev)
    c3 = sh(["python3", "tools/check_corpus_census.py"], timeout=900)
    if c3.returncode != 0:
        ev = ((c3.stdout or "") + (c3.stderr or ""))[-900:]
        return False, "census check failed (exit %d):\n%s" % (c3.returncode, ev)
    return True, ""


def diff_of(path: str, new_content: str) -> str:
    try:
        old = head(path)
    except RuntimeError:
        old = None
    if old is None:
        d = difflib.unified_diff([], new_content.splitlines(True),
                                 fromfile="/dev/null", tofile=f"b/{path}", lineterm="\n")
    else:
        d = difflib.unified_diff(old.splitlines(True), new_content.splitlines(True),
                                 fromfile=f"a/{path}", tofile=f"b/{path}", lineterm="\n")
    return "".join(d)


def _trace(issue_no: int, rec: dict) -> None:
    import json
    BRAIN_TRACE.mkdir(parents=True, exist_ok=True)
    p = BRAIN_TRACE / f"issue-{issue_no}.jsonl"
    with open(p, "a") as f:
        f.write(json.dumps({"t": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
                            **rec}) + "\n")


def propose(issue_no: int, round_no: int, repro_src: str, expected: str,
            feedback: str) -> str | None:
    """Round r = candidate r (deterministic descent). Returns a completion
    string with ```diff blocks, or a no-diff rationale."""
    fam = FAMILIES.get(expected) or ISSUE_FAMILY_HINTS.get(issue_no)
    if fam is None:
        _trace(issue_no, {"kind": "no-family", "expected": expected})
        return (f"specialized brain: no template family for {expected} — the "
                f"issue is outside the declared class templates (AUTHOR-STAGE "
                f"§5 anti-Goodhart binding). BLOCK, never hand-tailor.")
    import importlib
    mod = None
    try:
        mod = importlib.import_module(fam)
    except ImportError:
        mod = importlib.import_module(f"tools.{fam}")
    # families declare candidates_for(issue) when their class has per-issue
    # variants; the older families keep the plain candidates() descent.
    if hasattr(mod, "candidates_for"):
        cands = mod.candidates_for(issue_no)
    else:
        cands = mod.candidates()
    repro_rel = "compiler/target/author-stage/repro.hy"
    SMOKE_DIR.mkdir(parents=True, exist_ok=True)
    (ROOT / repro_rel).write_text(repro_src)

    idx = round_no - 1
    if idx >= len(cands):
        _trace(issue_no, {"kind": "descent-exhausted", "round": round_no})
        return (f"specialized brain: descent exhausted at round {round_no} — "
                f"{len(cands)} candidate(s) tried; see "
                f"target/author-brain-trace/issue-{issue_no}.jsonl. BLOCK.")

    cand = cands[idx]
    # NOTE: corpus-fixture registration (+ its census surfaces) is OUT of the
    # author stage's scope by design — the scope guard forbids root/CI-file
    # edits, and the harness requires a fixture only for NEW reject classes
    # (these families extend existing ones). surface_edits() remains a
    # session-side utility; a session may register a fixture post-landing.
    rec = {"kind": "candidate", "round": round_no, "family": fam,
           "variant": cand["name"]}
    try:
        apply_candidate(cand)
    except RuntimeError as e:
        rec["result"] = f"anchor-failure: {e}"
        _trace(issue_no, rec)
        revert_tree()
        return (f"specialized brain: template anchor rotted against HEAD — "
                f"{e} (candidate {cand['name']}). No diff proposed; the "
                f"family needs re-derivation against the new tree.")
    ok, evidence = smoke(repro_rel, expected, cand.get("rust_tests", []))
    rec["result"] = "green" if ok else f"smoke-fail: {evidence[:300]}"
    _trace(issue_no, rec)
    if not ok:
        revert_tree()
        return (f"specialized brain: candidate '{cand['name']}' failed "
                f"internal pre-verification — {evidence[:600]} No diff "
                f"proposed this round.")
    # green: capture diffs BEFORE reverting
    diffs = []
    touched = {p for p, *_ in cand["edits"]} | {p for p, _ in cand["new_files"]}
    for path in sorted(touched):
        diffs.append(diff_of(path, (ROOT / path).read_text()))
    revert_tree()
    rationale = cand["rationale"]
    return ("```diff\n" + "\n".join(diffs) + "\n```\n\n" + rationale)


def selftest() -> int:
    ok = True

    def check(name, cond):
        nonlocal ok
        print(("PASS " if cond else "FAIL ") + name)
        ok = ok and cond

    import importlib
    check("families registered", set(FAMILIES) == {"HYB-E6001", "HYB-E6006", "HYB-E2009", "HYB-E2002", "HYB-E1002"})
    check("issue hints registered", ISSUE_FAMILY_HINTS == {905: "sp_tmpl_ebind"})
    for fam in ("sp_tmpl_e6001", "sp_tmpl_e6006", "sp_tmpl_e2009", "sp_tmpl_ebind"):
        try:
            try:
                mod = importlib.import_module(fam)
            except ImportError:
                mod = importlib.import_module(f"tools.{fam}")
            cands = mod.candidates()
            n = sum(len(c["edits"]) + len(c["new_files"]) for c in cands)
            check(f"{fam}: {len(cands)} candidate(s), {n} edits+files", n > 0)
        except Exception as e:  # noqa: BLE001
            check(f"{fam} import failed: {e}", False)
    print("SELFTEST " + ("PASS" if ok else "FAIL"))
    return 0 if ok else 1


if __name__ == "__main__":
    import sys
    sys.exit(selftest())
