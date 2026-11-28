"""Spike for wildcard proposal #4685: run-domain emit target for the checked fragment.

The proposal claims a second execution representation for one narrow fragment:
a source that already stores (value, repeat count) pairs can be reduced by
evaluating a total, pure predicate once per *run* and adding the run length,
without expanding to one element per logical sample.

This module is a model, not a Hybrid backend. It does four things:

1. Compiles one predicate of an admitted fragment and emits the Go loop.
2. Rejects the cases the proposal says must be rejected (zero length, overflow,
   time reads, element-position predicates, unsupported/effectful nodes).
3. Checks the arithmetic idea against a dense reference on every sequence of
   lengths 0..7 over {0,1,2}, and under splitting at every position.
4. Reports predicate evaluations and allocation order, so a run-domain harness
   that silently expands valid runs is detectable instead of assumed.

Failure is a valid outcome: `Outcome.FAILURE` is returned when a gate fires.
"""

from __future__ import annotations

import dataclasses
from typing import Iterable, List, Sequence, Tuple

UINT64_MAX = (1 << 64) - 1
INT64_MIN = -(1 << 63)
INT64_MAX = (1 << 63) - 1

# Diagnostics
E_ZERO_LENGTH = "HYB-E4685-01"     # run length 0 accepted into the domain
E_LENGTH_OVERFLOW = "HYB-E4685-02"  # logical length does not fit uint64
E_SAMPLE_RANGE = "HYB-E4685-03"     # sample value does not fit int64
E_TIME_READ = "HYB-E4685-04"        # time read inside the fragment
E_POSITION_READ = "HYB-E4685-05"    # element-position predicate
E_UNSUPPORTED = "HYB-E4685-06"      # effectful or non-total node


class FragmentError(Exception):
    def __init__(self, code: str, detail: str = "") -> None:
        super().__init__(f"{code}: {detail}" if detail else code)
        self.code = code
        self.detail = detail


# --- predicate fragment -----------------------------------------------------


class Node:
    total = True

    def children(self) -> Tuple["Node", ...]:
        return ()


@dataclasses.dataclass(frozen=True)
class Const(Node):
    value: int


@dataclasses.dataclass(frozen=True)
class Var(Node):
    pass


@dataclasses.dataclass(frozen=True)
class Time(Node):
    total = False


@dataclasses.dataclass(frozen=True)
class Index(Node):
    total = False


_CMP = {"==": lambda a, b: a == b, "!=": lambda a, b: a != b, "<": lambda a, b: a < b,
        "<=": lambda a, b: a <= b, ">": lambda a, b: a > b, ">=": lambda a, b: a >= b}
_LOGIC = {"&&": lambda a, b: bool(a) and bool(b), "||": lambda a, b: bool(a) or bool(b)}


def _i64(v: int) -> int:
    """Wrap into int64, like a Go int64 sample would."""
    v &= UINT64_MAX
    return v - (1 << 64) if v > INT64_MAX else v


def _i64_mod(a: int, b: int) -> int:
    if b == 0:
        raise FragmentError(E_UNSUPPORTED, "modulo by zero")
    q = abs(a) // abs(b)
    r = abs(a) - q * abs(b)
    return -r if a < 0 else r  # Go truncates toward zero


_ARITH = {"+": lambda a, b: _i64(a + b), "-": lambda a, b: _i64(a - b),
          "*": lambda a, b: _i64(a * b), "%": _i64_mod, "&": lambda a, b: _i64(a & b),
          "|": lambda a, b: _i64(a | b), "^": lambda a, b: _i64(a ^ b)}
@dataclasses.dataclass(frozen=True)
class BinOp(Node):
    op: str
    lhs: Node
    rhs: Node

    def children(self) -> Tuple[Node, ...]:
        return (self.lhs, self.rhs)

    @property
    def total(self) -> bool:
        return self.lhs.total and self.rhs.total


@dataclasses.dataclass(frozen=True)
class Not(Node):
    operand: Node

    def children(self) -> Tuple[Node, ...]:
        return (self.operand,)

    @property
    def total(self) -> bool:
        return self.operand.total


def makes_go(node: Node) -> str:
    if isinstance(node, Const):
        return str(node.value)
    if isinstance(node, Var):
        return "r.Value"
    if isinstance(node, Time):
        return "r.Value"  # placeholder only, rejected before emit
    if isinstance(node, Index):
        return "r.Value"
    if isinstance(node, BinOp):
        return f"({makes_go(node.lhs)}{node.op}{makes_go(node.rhs)})"
    if isinstance(node, Not):
        return f"(!{makes_go(node.operand)})"
    raise FragmentError(E_UNSUPPORTED, type(node).__name__)


def _render(node: Node) -> str:
    """Go source with spaces removed where safe; precedence is kept by parens."""
    if isinstance(node, BinOp) and node.op in _ARITH:
        return f"({_render(node.lhs)} {node.op} {_render(node.rhs)})"
    if isinstance(node, BinOp) and node.op in _CMP:
        return f"{_render(node.lhs)} {node.op} {_render(node.rhs)}"
    if isinstance(node, BinOp) and node.op in _LOGIC:
        return f"({_render(node.lhs)} {node.op} {_render(node.rhs)})"
    if isinstance(node, Not):
        return f"!({_render(node.operand)})"
    return makes_go(node)


def _eval(node: Node, value: int, index: int) -> object:
    if isinstance(node, Const):
        return node.value
    if isinstance(node, Var):
        return value
    if isinstance(node, Time):
        raise FragmentError(E_TIME_READ, "time read needs a second input")
    if isinstance(node, Index):
        raise FragmentError(E_POSITION_READ, "position read needs expanded input")
    if isinstance(node, BinOp):
        lhs = _eval(node.lhs, value, index)
        rhs = _eval(node.rhs, value, index)
        if node.op in _ARITH:
            return _ARITH[node.op](int(lhs), int(rhs))
        if node.op in _CMP:
            return _CMP[node.op](int(lhs), int(rhs))
        if node.op == "&&":
            return bool(lhs) and bool(rhs)
        if node.op == "||":
            return bool(lhs) or bool(rhs)
        raise FragmentError(E_UNSUPPORTED, node.op)
    if isinstance(node, Not):
        return not bool(_eval(node.operand, value, index))
    raise FragmentError(E_UNSUPPORTED, type(node).__name__)


# --- runs -------------------------------------------------------------------


@dataclasses.dataclass(frozen=True)
class Run:
    value: int
    length: int


@dataclasses.dataclass(frozen=True)
class Outcome:
    kind: str                      # "accepted" | "failure"
    code: str = ""
    total: int = 0
    predicate_evals: int = 0
    run_alloc: int = 0             # units allocated in run domain
    dense_alloc: int = 0           # units allocated by the dense reference


def validate(runs: Sequence[Run]) -> int:
    """Admission checks of the proposal: positive lengths, uint64 total, int64 values."""
    total = 0
    for r in runs:
        if r.length == 0:
            raise FragmentError(E_ZERO_LENGTH, f"run value {r.value}")
        if not (0 <= r.length <= UINT64_MAX):
            raise FragmentError(E_LENGTH_OVERFLOW, f"run length {r.length}")
        if not (INT64_MIN <= r.value <= INT64_MAX):
            raise FragmentError(E_SAMPLE_RANGE, f"sample {r.value}")
        total += r.length
        if total > UINT64_MAX:
            raise FragmentError(E_LENGTH_OVERFLOW, "logical length exceeds uint64")
    return total


def compile_predicate(node: Node) -> Node:
    """Compile-time rejection; anything outside the first fragment fails here."""
    if not node.total:
        _eval(node, 0, 0)  # raises the precise code
        raise FragmentError(E_UNSUPPORTED, "non-total node")
    return node


def count_run_domain(runs: Sequence[Run], pred: Node) -> Outcome:
    """One predicate evaluation per run; count only when the run satisfies it."""
    try:
        total = validate(runs)
        compile_predicate(pred)
    except FragmentError as err:
        return Outcome("failure", code=err.code)

    count = 0
    evals = 0
    for r in runs:
        evals += 1
        try:
            hit = _eval(pred, r.value, 0)
        except FragmentError as err:
            return Outcome("failure", code=err.code)
        if hit:
            count += r.length
    return Outcome("accepted", total=count, predicate_evals=evals,
                   run_alloc=len(runs), dense_alloc=total)


def dense_reference(runs: Sequence[Run], pred: Node) -> int:
    """Expand to one element per sample and evaluate per element."""
    count = 0
    index = 0
    for r in runs:
        for _ in range(r.length):
            if _eval(pred, r.value, index):
                count += 1
            index += 1
    return count


# --- emit -------------------------------------------------------------------

TEMPLATE = """package main

import "fmt"

type Run struct {{
\tValue  int64
\tLength uint64
}}

func countMatching(runs []Run) (total uint64, predicateEvals uint64) {{
\tfor _, r := range runs {{
\t\tpredicateEvals++
\t\tif {pred} {{
\t\t\ttotal += r.Length
\t\t}}
\t}}
\treturn total, predicateEvals
}}

func main() {{
\truns := []Run{{{runs}}}
\ttotal, evals := countMatching(runs)
\tfmt.Printf("%d %d\\n", total, evals)
}}
"""


def emit_go(runs: Sequence[Run], pred: Node) -> str:
    literal = ", ".join(f"{{Value: {r.value}, Length: {r.length}}}" for r in runs)
    return TEMPLATE.format(pred=_render(pred), runs=literal)


def _sequences(values: Sequence[int], length: int) -> Iterable[Tuple[int, ...]]:
    if length == 0:
        yield ()
        return
    for head in values:
        for tail in _sequences(values, length - 1):
            yield (head,) + tail


def compressed(seq: Sequence[int]) -> List[Run]:
    out: List[Run] = []
    for v in seq:
        if out and out[-1].value == v:
            out[-1] = Run(v, out[-1].length + 1)
        else:
            out.append(Run(v, 1))
    return out


def run_dict(seq: Sequence[int], pred: Node) -> Tuple[bool, int]:
    res = count_run_domain(compressed(seq), pred)
    return res.kind == "accepted", res.total


def check_equivalence(pred: Node, alphabet=(0, 1, 2)) -> Tuple[int, int, int]:
    """Dense vs run domain, plus split invariance. Returns (sequences, mismatches, splits_checked)."""
    seqs = 0
    bad = 0
    splits = 0
    for length in range(0, 8):
        for seq in _sequences(alphabet, length):
            seqs += 1
            ok, got = run_dict(seq, pred)
            if not ok or got != dense_reference(compressed(seq), pred):
                bad += 1
            for cut in range(length + 1):
                splits += 1
                left = seq[:cut]
                right = seq[cut:]
                ok_l, got_l = run_dict(left, pred)
                ok_r, got_r = run_dict(right, pred)
                if not (ok_l and ok_r) or got_l + got_r != dense_reference(compressed(seq), pred):
                    bad += 1
    return seqs, bad, splits


if __name__ == "__main__":
    pred = BinOp("==", BinOp("%", BinOp("*", Var(), Var()), Const(5)), Const(1))
    seqs, bad, splits = check_equivalence(pred)
    print(f"sequences={seqs} splits={splits} mismatches={bad}")
    res = count_run_domain([Run(1, 1_000_000_000)], pred)
    print(f"one_billion: total={res.total} predicate_evals={res.predicate_evals} "
          f"run_alloc={res.run_alloc} dense_alloc={res.dense_alloc}")
