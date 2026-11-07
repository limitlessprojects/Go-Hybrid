#!/usr/bin/env python3
"""M1/M2 Verifier for Wildcard Spike: Linear Must-Consume Obligations & Escape-Certified Stack Regions.

Checks:
- HYB-E3701: Unconsumed linear handle obligation on any control-flow path (branch drop, early `?` try-return without `defer`, or function exit).
- HYB-E3702: Interface / heap-escape sink inside an `@[no_escape_loop]` region (Falsifier F2 guard).
"""
import argparse
import pathlib
import re
import subprocess
import sys
from dataclasses import dataclass
from typing import List


@dataclass
class Diagnostic:
    code: str
    line: int
    message: str


CONSUMING_PATTERNS = (
    re.compile(r"\b(\w+)\.(?:finish|close|consume)\s*\("),
    re.compile(r"\bmove\s+(\w+)\b"),
)
HANDLE_INIT_RE = re.compile(r"^\s*(\w+)\s*:=\s*(?:flate2\.new_\w+|AcquireHandle)\s*\(")
DEFER_CONSUME_RE = re.compile(r"^\s*defer\s+(\w+)\.(?:finish|close|consume)\s*\(")
TRY_OP_RE = re.compile(r"\?\s*$")
INTERFACE_SINK_RE = re.compile(r"\b(?:sink\.Observe|fmt\.Sprint|fmt\.Println)\s*\(\s*&(\w+)\s*\)")


def verify_hy_source(source: str) -> List[Diagnostic]:
    diags: List[Diagnostic] = []
    lines = source.splitlines()
    is_linear = "@[linear_must_consume]" in source
    is_noescape = "@[no_escape_loop]" in source

    if is_linear:
        handles = {}
        deferred = set()
        consumed_in_if = set()
        consumed_in_else = set()
        consumed_top = set()
        in_if = False
        in_else = False
        has_else = False

        for idx, raw in enumerate(lines, start=1):
            line = raw.strip()
            if not line or line.startswith("//"):
                continue

            m_init = HANDLE_INIT_RE.match(raw)
            if m_init:
                handles[m_init.group(1)] = idx
                continue

            m_def = DEFER_CONSUME_RE.match(raw)
            if m_def:
                deferred.add(m_def.group(1))
                continue

            if line.startswith("if ") and line.endswith("{"):
                in_if = True
                in_else = False
                continue
            if line.startswith("}") and "else" in line and line.endswith("{"):
                in_if = False
                in_else = True
                has_else = True
                continue
            if line == "}":
                in_if = False
                in_else = False
                continue

            # Check consuming operations
            consumed_here = set()
            for pat in CONSUMING_PATTERNS:
                for m in pat.finditer(line):
                    consumed_here.add(m.group(1))

            if consumed_here:
                for h in consumed_here:
                    if in_if:
                        consumed_in_if.add(h)
                    elif in_else:
                        consumed_in_else.add(h)
                    else:
                        consumed_top.add(h)

            # Early `?` try-return before handle is consumed or deferred
            if TRY_OP_RE.search(line) and not consumed_here:
                for h, h_line in handles.items():
                    if h not in deferred and h not in consumed_top:
                        diags.append(
                            Diagnostic(
                                code="HYB-E3701",
                                line=idx,
                                message=(
                                    f"early `?` return at line {idx} may exit without consuming linear handle `{h}` "
                                    f"(acquired at line {h_line})"
                                ),
                            )
                        )

        for h, h_line in handles.items():
            if h in deferred or h in consumed_top:
                continue
            if has_else and (h in consumed_in_if) and (h in consumed_in_else):
                continue
            diags.append(
                Diagnostic(
                    code="HYB-E3701",
                    line=h_line,
                    message=f"linear handle `{h}` is not consumed on all control-flow paths before function exit",
                )
            )

    if is_noescape:
        for idx, raw in enumerate(lines, start=1):
            m_esc = INTERFACE_SINK_RE.search(raw)
            if m_esc:
                diags.append(
                    Diagnostic(
                        code="HYB-E3702",
                        line=idx,
                        message=(
                            f"address `&{m_esc.group(1)}` passed to dynamic interface sink inside `@[no_escape_loop]` "
                            f"(triggers Go cmd/compile heap escape; Falsifier F2)"
                        ),
                    )
                )

    return diags


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--check", action="store_true", help="Verify .hy fixtures")
    parser.add_argument("--live-go", action="store_true", help="Run live Go tests")
    args = parser.parse_args()

    root = pathlib.Path(__file__).resolve().parent
    if args.check:
        p1 = (root / "hy" / "01-linear-branch-complete-admitted.hy").read_text()
        p2 = (root / "hy" / "02-linear-early-try-leak-rejected.hy").read_text()
        p3 = (root / "hy" / "03-noescape-loop-reuse-admitted.hy").read_text()
        assert len(verify_hy_source(p1)) == 0, "01 should be admitted"
        d2 = verify_hy_source(p2)
        assert len(d2) == 2 and all(d.code == "HYB-E3701" for d in d2), f"02 should emit 2 HYB-E3701, got {d2}"
        assert len(verify_hy_source(p3)) == 0, "03 should be admitted"
        print("[PASS] H1/H2 static .hy fixture verification (01 ACCEPT, 02 REJECT 2x HYB-E3701, 03 ACCEPT)")

    if args.live_go:
        subprocess.run(["go", "test", "-v", "./..."], cwd=root / "work", check=True)
        print("[PASS] H3 live Go parity & zero-alloc verification")

    return 0


if __name__ == "__main__":
    sys.exit(main())
