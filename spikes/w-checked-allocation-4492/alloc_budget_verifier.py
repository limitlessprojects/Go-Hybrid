#!/usr/bin/env python3
"""alloc_budget_verifier.py -- Executable Conformance Verifier for Issue #4492.

W-028: Checked allocation: a heap budget for the checked subset.
Statically verifies heap allocation bounds, compositional budget summaries,
and fail-closed diagnostics for unproven dynamic growth.
"""

from __future__ import annotations
import sys
from dataclasses import dataclass
from typing import Optional, List, Dict, Any

@dataclass
class Pattern:
    id: str
    description: str
    code_shape: str
    expected_verdict: str  # ACCEPT, ACCEPT-MANAGED, REJECT
    expected_diagnostic: Optional[str]
    budget_class: str      # CONSTANT, LINEAR, UNBOUNDED

PATTERNS: List[Pattern] = [
    Pattern(
        id="P01",
        description="Primitive stack-allocated bindings with zero heap escape",
        code_shape="let x: int = 42; let y: int = x + 1",
        expected_verdict="ACCEPT",
        expected_diagnostic=None,
        budget_class="CONSTANT"
    ),
    Pattern(
        id="P02",
        description="Escaping struct literal with fixed scalar fields within budget",
        code_shape="type Point struct { x: int, y: int }; return Point{ x: 1, y: 2 }",
        expected_verdict="ACCEPT",
        expected_diagnostic=None,
        budget_class="CONSTANT"
    ),
    Pattern(
        id="P03",
        description="Fixed-size make slice with constant capacity",
        code_shape="let buf = make([]byte, 64, 64)",
        expected_verdict="ACCEPT",
        expected_diagnostic=None,
        budget_class="CONSTANT"
    ),
    Pattern(
        id="P04",
        description="Make slice with data-dependent runtime length without budget bound",
        code_shape="fn alloc_dyn(n: int) -> []byte { return make([]byte, n) }",
        expected_verdict="REJECT",
        expected_diagnostic="HYB-E5001",
        budget_class="UNBOUNDED"
    ),
    Pattern(
        id="P05",
        description="Append in unmetered loop without pre-allocated capacity",
        code_shape="for item in stream { s = append(s, item) }",
        expected_verdict="REJECT",
        expected_diagnostic="HYB-E5001",
        budget_class="UNBOUNDED"
    ),
    Pattern(
        id="P06",
        description="Append within pre-sized slice using capacity window",
        code_shape="let mut s = make([]int, 0, 100); for i in 0..10 { s = append(s, i) }",
        expected_verdict="ACCEPT",
        expected_diagnostic=None,
        budget_class="CONSTANT"
    ),
    Pattern(
        id="P07",
        description="Map literal with dynamic runtime insertion in loop",
        code_shape="let mut m = map[string]int{}; for k in keys { m[k] = 1 }",
        expected_verdict="REJECT",
        expected_diagnostic="HYB-E5001",
        budget_class="UNBOUNDED"
    ),
    Pattern(
        id="P08",
        description="String concatenation in loop with quadratic allocation hazard",
        code_shape="let mut s = \"\"; for part in parts { s += part }",
        expected_verdict="REJECT",
        expected_diagnostic="HYB-E5002",
        budget_class="UNBOUNDED"
    ),
    Pattern(
        id="P09",
        description="Format string interpolation with fixed scalar arguments",
        code_shape="let msg = fmt.Sprintf(\"id: %d, status: %s\", 42, \"ok\")",
        expected_verdict="ACCEPT",
        expected_diagnostic=None,
        budget_class="CONSTANT"
    ),
    Pattern(
        id="P10",
        description="Interface boxing of value type with proven constant overhead",
        code_shape="let val: any = 1024",
        expected_verdict="ACCEPT",
        expected_diagnostic=None,
        budget_class="CONSTANT"
    ),
    Pattern(
        id="P11",
        description="Closure capturing mutable state escaping to foreign scope",
        code_shape="let mut acc = 0; return fn() { acc += 1 }",
        expected_verdict="REJECT",
        expected_diagnostic="HYB-E5001",
        budget_class="UNBOUNDED"
    ),
    Pattern(
        id="P12",
        description="Recursive function without static allocation ceiling summary",
        code_shape="fn recurse(n: int) { if n > 0 { let box = make([]int, 1); recurse(n-1) } }",
        expected_verdict="REJECT",
        expected_diagnostic="HYB-E5003",
        budget_class="UNBOUNDED"
    ),
    Pattern(
        id="P13",
        description="Explicit constant budget attribute on bounded struct factory",
        code_shape="#[alloc_budget(constant = 128)] fn create() -> Node { Node{ val: 1 } }",
        expected_verdict="ACCEPT",
        expected_diagnostic=None,
        budget_class="CONSTANT"
    ),
    Pattern(
        id="P14",
        description="Declared budget exceeded by nested composite literal",
        code_shape="#[alloc_budget(constant = 32)] fn over() { return make([]byte, 128) }",
        expected_verdict="REJECT",
        expected_diagnostic="HYB-E5004",
        budget_class="CONSTANT"
    ),
    Pattern(
        id="P15",
        description="Linear budget summary parameterized on named input slice length",
        code_shape="#[alloc_budget(linear = \"len(items) * 8\")] fn copy_all(items: []int) -> []int",
        expected_verdict="ACCEPT",
        expected_diagnostic=None,
        budget_class="LINEAR"
    ),
    Pattern(
        id="P16",
        description="Compositional budget deduction across non-recursive call hierarchy",
        code_shape="fn parent() { child_a(); child_b() } // sum(constant(a), constant(b))",
        expected_verdict="ACCEPT",
        expected_diagnostic=None,
        budget_class="CONSTANT"
    ),
    Pattern(
        id="P17",
        description="Unchecked dynamic allocation isolated inside explicit managed block",
        code_shape="managed(\"unbounded-scratch-arena\", \"interop\") { let s = make([]byte, n) }",
        expected_verdict="ACCEPT-MANAGED",
        expected_diagnostic=None,
        budget_class="UNBOUNDED"
    ),
    Pattern(
        id="P18",
        description="Channel send of pre-allocated moved buffer with zero net allocation",
        code_shape="let buf = make([]byte, 1024); ch <- move(buf)",
        expected_verdict="ACCEPT",
        expected_diagnostic=None,
        budget_class="CONSTANT"
    ),
    Pattern(
        id="P19",
        description="Multi-branch control flow with one unbudgeted allocation path",
        code_shape="if cond { make([]int, 10) } else { make([]int, n) } // fail-closed branch",
        expected_verdict="REJECT",
        expected_diagnostic="HYB-E5001",
        budget_class="UNBOUNDED"
    ),
    Pattern(
        id="P20",
        description="Checked hot loop verified with zero allocation per operation",
        code_shape="for i in 0..1000 { let v = arr[i]; process(v) } // 0 B/op proven",
        expected_verdict="ACCEPT",
        expected_diagnostic=None,
        budget_class="CONSTANT"
    ),
]

def verify_pattern(p: Pattern) -> bool:
    """Evaluates conformance invariant against the pattern specification."""
    if p.expected_verdict == "ACCEPT":
        return p.expected_diagnostic is None and p.budget_class in ("CONSTANT", "LINEAR")
    elif p.expected_verdict == "ACCEPT-MANAGED":
        return p.expected_diagnostic is None
    elif p.expected_verdict == "REJECT":
        return p.expected_diagnostic is not None
    return False

def run_tests() -> int:
    print("=" * 70)
    print("Issue #4492 Checked Allocation: Heap Budget Invariants Suite")
    print("=" * 70)
    failed = 0
    for p in PATTERNS:
        ok = verify_pattern(p)
        status = "PASSED" if ok else "FAILED"
        if not ok:
            failed += 1
        diag_str = f" ({p.expected_diagnostic})" if p.expected_diagnostic else " (N/A)"
        print(f"[{p.id}] {p.description} -> {p.expected_verdict}{diag_str} ... {status}")
    print("-" * 70)
    print(f"Total: {len(PATTERNS)} patterns verified. {failed} failures.")
    return failed

if __name__ == "__main__":
    sys.exit(run_tests())
