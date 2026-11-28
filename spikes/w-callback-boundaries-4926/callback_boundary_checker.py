#!/usr/bin/env python3
"""
Standalone spike checker for Issue #4926:
[wildcard] Make callback boundaries unobservable for finite-state stream kernels.

The proposal asks for an opt-in contract: a streaming kernel may be called
with any division of the stream into buffers (callbacks) without changing
the values emitted, their order, or the residual state a later call
observes. This spike is the "review spike" the issue describes, built as a
library-level checker that does not modify the Hybrid language:

- A kernel inside the fragment is a finite transducer: a finite state set,
  a finite input alphabet, at most one emission per consumed sample, and
  an explicit boundary transition describing what a callback boundary does
  to the state. No flush output (a flush is a separate operation, excluded
  from the fragment, exactly as the issue scopes it).
- The checker explores reachable pairs of states for an unbroken execution
  (one callback) versus an execution that admits arbitrary boundaries.
  If no reachable pair can diverge, the kernel is CERTIFIED inside the
  fragment. If a pair diverges, the checker returns a short concrete
  witness: an input and a cut pattern whose two runs a kernel author can
  replay. Kernels outside the fragment get no certificate and no
  rejection - the checker declines to judge them.

First kernel, from the issue: an audio decimator emitting samples
0, 3, 6, ... (stride 3) with explicit phase. Carrying the phase across
callbacks must certify; resetting the phase at every callback must be
rejected with the issue's minimal witness class ([0] | [1]: one batch
emits [0], restarting emits [0, 1]).

The empirical leg reproduces the issue's independent probe exactly:
every binary input of lengths 1 through 8 under every nonempty-block
partition (43,690 cases; carry misses 0; reset misses 41,084 output
traces and 41,916 index-tagged traces), the empty-callback law for all
three phases, and the 48,000-sample ramp counts (batch 16,000; restart
at fixed widths 96 / 127 / 128 / 1,024 gives 16,000 / 16,252 / 16,125 /
16,031).

Run directly to print verdicts and probe numbers; the process exits
nonzero if any reproduced number disagrees with the issue.
"""

from collections import deque
from itertools import product

STRIDE = 3
ALPHABET = (0, 1)


# ---------------------------------------------------------------------------
# Block-level oracle: the semantics of the issue's independent probe.
# ---------------------------------------------------------------------------

def step_carry(q, block):
    """One callback of the decimator with the phase carried in."""
    emitted = [v for i, v in enumerate(block) if (q + i) % STRIDE == 0]
    return (q + len(block)) % STRIDE, emitted


def step_reset(q, block):
    """One callback of the decimator with the phase reset at each call."""
    emitted = [v for i, v in enumerate(block) if i % STRIDE == 0]
    return len(block) % STRIDE, emitted


def run_blocks_oracle(values, blocks, reset):
    """Fold a block step over a partition, starting from phase 0."""
    step = step_reset if reset else step_carry
    q, out = 0, []
    for block in blocks:
        q, emitted = step(q, block)
        out.extend(emitted)
    return q, out


def partitions_from_mask(values, mask):
    """Split values into nonempty blocks at the set bits of mask."""
    edges = [0] + [i for i in range(1, len(values)) if mask & (1 << (i - 1))]
    edges.append(len(values))
    return [values[a:b] for a, b in zip(edges, edges[1:])]


# ---------------------------------------------------------------------------
# Finite-transducer kernel model (the fragment).
# ---------------------------------------------------------------------------

class Transducer:
    """A kernel declared inside the checkable fragment.

    states: finite iterable of states.
    alphabet: finite iterable of input symbols.
    consume(state, symbol) -> (next_state, emitted tuple of length 0 or 1)
        One sample inside a callback.
    boundary(state) -> state
        What a callback boundary does to the state before the next
        callback's first sample. Identity means the boundary is invisible
        to the state; a reset maps every state to the initial phase.
    has_flush: True if the kernel emits at a final flush (outside the
        fragment; the flush is a separate operation).
    unbounded_state: True if the state set is not finite (outside the
        fragment).
    """

    def __init__(self, name, states, alphabet, consume, boundary,
                 has_flush=False, unbounded_state=False):
        self.name = name
        self.states = states
        self.alphabet = alphabet
        self.consume = consume
        self.boundary = boundary
        self.has_flush = has_flush
        self.unbounded_state = unbounded_state


def make_decimator(mode, stride=STRIDE):
    """The issue's decimator as a transducer. mode: 'carry' or 'reset'."""
    if mode not in ("carry", "reset"):
        raise ValueError(mode)

    def consume(q, a):
        return (q + 1) % stride, ((a,) if q == 0 else ())

    def boundary(q):
        return q if mode == "carry" else 0

    return Transducer(
        name="decimator-%s" % mode,
        states=tuple(range(stride)),
        alphabet=ALPHABET,
        consume=consume,
        boundary=boundary,
    )


def run_blocks_transducer(kernel, q0, blocks):
    """Fold a transducer over a partition: boundary before each block."""
    q, out = q0, []
    for block in blocks:
        q = kernel.boundary(q)
        for a in block:
            q, emitted = kernel.consume(q, a)
            out.extend(emitted)
    return q, out


# ---------------------------------------------------------------------------
# The checker: reachable-pair exploration.
# ---------------------------------------------------------------------------

CERTIFIED = "CERTIFIED"
REJECTED = "REJECTED"
OUTSIDE_FRAGMENT = "OUTSIDE_FRAGMENT"


class Verdict:
    def __init__(self, kind, kernel_name, pairs_explored=0, witness=None,
                 reason=None):
        self.kind = kind
        self.kernel_name = kernel_name
        self.pairs_explored = pairs_explored
        self.witness = witness
        self.reason = reason

    def __repr__(self):
        return "Verdict(%s, %s)" % (self.kind, self.kernel_name)


def _fragment_violation(kernel):
    """Return a reason string if the kernel is outside the fragment."""
    if kernel.unbounded_state:
        return "state set is not finite"
    if kernel.has_flush:
        return "kernel emits at a final flush (flush is a separate operation)"
    if kernel.states is None:
        return "no finite state set declared"
    states = list(kernel.states)
    if not states:
        return "empty state set"
    state_set = set(states)
    for q in states:
        if kernel.boundary(q) not in state_set:
            return "boundary transition leaves the declared state set"
        for a in kernel.alphabet:
            q2, emitted = kernel.consume(q, a)
            if q2 not in state_set:
                return "consume transition leaves the declared state set"
            if len(emitted) > 1:
                return "more than one emission per sample"
    return None


def _reconstruct(parents, node):
    """Rebuild (input, cut positions) from the BFS parent chain."""
    symbols, cuts = [], []
    events = []
    while parents[node] is not None:
        node, event = parents[node]
        events.append(event)
    events.reverse()
    consumed = 0
    for event in events:
        if event[0] == "consume":
            symbols.append(event[1])
            consumed += 1
        else:  # boundary event on the admitting side
            cuts.append(consumed)
    return symbols, cuts


def _partition(values, cuts):
    """Blocks of values cut after the given positions (empties kept)."""
    blocks, start = [], 0
    for cut in cuts:
        blocks.append(values[start:cut])
        start = cut
    blocks.append(values[start:])
    return blocks


def _witness_dict(kernel, symbols, cuts, kind, detail):
    blocks = _partition(list(symbols), cuts)
    unbroken_q, unbroken_out = run_blocks_transducer(kernel, 0, [list(symbols)])
    chunked_q, chunked_out = run_blocks_transducer(kernel, 0, blocks)
    return {
        "kind": kind,
        "input": list(symbols),
        "blocks": blocks,
        "unbroken_emissions": unbroken_out,
        "chunked_emissions": chunked_out,
        "unbroken_final_state": unbroken_q,
        "chunked_final_state": chunked_q,
        "detail": detail,
    }


def certify(kernel):
    """Decide boundary-unobservability for one kernel inside the fragment.

    Explores pairs (unbroken state, boundary-admitting state) reachable
    while both executions consume the same input and have emitted the
    same values so far. A consume step that emits differently on the two
    sides, or a reachable pair whose residual states differ, is a
    violation; the first (shortest-input) emission violation is reported
    as the witness when one exists.
    """
    violation = _fragment_violation(kernel)
    if violation is not None:
        return Verdict(OUTSIDE_FRAGMENT, kernel.name, reason=violation)

    starts = []
    for q0 in kernel.states:
        start = (kernel.boundary(q0), kernel.boundary(q0))
        if start not in starts:
            starts.append(start)

    parents = {}
    queue = deque()
    for start in starts:
        parents[start] = None
        queue.append(start)

    residual_witness = None
    explored = 0
    while queue:
        pair = queue.popleft()
        explored += 1
        qu, qa = pair
        if qu != qa and residual_witness is None:
            symbols, cuts = _reconstruct(parents, pair)
            residual_witness = _witness_dict(
                kernel, symbols, cuts, "residual",
                "residual states differ after the same input: "
                "unbroken %r vs boundary-admitting %r" % (qu, qa))
        # Boundary transitions on the admitting side only.
        qa2 = kernel.boundary(qa)
        if qa2 != qa:
            nxt = (qu, qa2)
            if nxt not in parents:
                parents[nxt] = (pair, ("boundary",))
                queue.append(nxt)
        # Consume the next sample on both sides.
        for a in kernel.alphabet:
            qu2, eu = kernel.consume(qu, a)
            qa2, ea = kernel.consume(qa, a)
            if eu != ea:
                symbols, cuts = _reconstruct(parents, pair)
                witness = _witness_dict(
                    kernel, symbols + [a], cuts, "emission",
                    "emissions diverge on sample %r: unbroken emitted "
                    "%r, boundary-admitting emitted %r" % (a, eu, ea))
                return Verdict(REJECTED, kernel.name, explored, witness)
            nxt = (qu2, qa2)
            if nxt not in parents:
                parents[nxt] = (pair, ("consume", a))
                queue.append(nxt)

    if residual_witness is not None:
        return Verdict(REJECTED, kernel.name, explored, residual_witness)
    return Verdict(CERTIFIED, kernel.name, explored)


# ---------------------------------------------------------------------------
# Kernels outside the fragment (the checker must decline both).
# ---------------------------------------------------------------------------

def make_counter_kernel():
    """Emits every 7th sample overall; needs an unbounded counter state."""
    return Transducer(
        name="global-counter",
        states=None,
        alphabet=ALPHABET,
        consume=lambda q, a: (q + 1, (a,) if q % 7 == 6 else ()),
        boundary=lambda q: q,
        unbounded_state=True,
    )


def make_flush_kernel():
    """Decimator plus a flush that emits a marker for a partial group."""
    base = make_decimator("carry")
    return Transducer(
        name="decimator-with-flush",
        states=base.states,
        alphabet=base.alphabet,
        consume=base.consume,
        boundary=base.boundary,
        has_flush=True,
    )


# ---------------------------------------------------------------------------
# The empirical leg: reproduce the issue's independent probe.
# ---------------------------------------------------------------------------

def probe_exhaustive():
    """Binary inputs of lengths 1-8 under every nonempty-block partition."""
    cases = carry_misses = reset_misses = tagged_changes = 0
    for n in range(1, 9):
        for values in product((0, 1), repeat=n):
            values = list(values)
            expected = values[::STRIDE]
            tagged = list(range(n))
            expected_tagged = tagged[::STRIDE]
            for mask in range(1 << (n - 1)):
                cases += 1
                blocks = partitions_from_mask(values, mask)
                if run_blocks_oracle(values, blocks, reset=False) != (n % STRIDE, expected):
                    carry_misses += 1
                if run_blocks_oracle(values, blocks, reset=True)[1] != expected:
                    reset_misses += 1
                tagged_blocks = partitions_from_mask(tagged, mask)
                if run_blocks_oracle(tagged, tagged_blocks, reset=True)[1] != expected_tagged:
                    tagged_changes += 1
    return {
        "cases": cases,
        "carry_misses": carry_misses,
        "reset_misses": reset_misses,
        "tagged_changes": tagged_changes,
    }


def probe_empty_callbacks():
    """Empty callbacks: carry preserves every phase; reset does not."""
    carry_ok = all(step_carry(q, []) == (q, []) for q in range(STRIDE))
    reset_violations = [q for q in range(STRIDE) if step_reset(q, []) != (q, [])]
    return {"carry_preserves": carry_ok, "reset_violating_phases": reset_violations}


def probe_ramp(total=48000, widths=(96, 127, 128, 1024)):
    """Emission counts for a long ramp: batch vs fixed-width restarts."""
    values = list(range(total))
    batch = len(values[::STRIDE])
    restarts = {}
    for w in widths:
        blocks = [values[i:i + w] for i in range(0, total, w)]
        restarts[w] = len(run_blocks_oracle(values, blocks, reset=True)[1])
    return {"batch": batch, "restarts": restarts}


EXPECTED_PROBE = {
    "cases": 43690,
    "carry_misses": 0,
    "reset_misses": 41084,
    "tagged_changes": 41916,
}
EXPECTED_RAMP = {"batch": 16000,
                 "restarts": {96: 16000, 127: 16252, 128: 16125, 1024: 16031}}


def main():
    ok = True

    for mode in ("carry", "reset"):
        verdict = certify(make_decimator(mode))
        print("%s: %s (pairs explored: %d)"
              % (verdict.kernel_name, verdict.kind, verdict.pairs_explored))
        if verdict.witness is not None:
            w = verdict.witness
            print("  witness (%s): input %s as blocks %s"
                  % (w["kind"], w["input"], w["blocks"]))
            print("  unbroken emissions %s (final state %s); "
                  "chunked emissions %s (final state %s)"
                  % (w["unbroken_emissions"], w["unbroken_final_state"],
                     w["chunked_emissions"], w["chunked_final_state"]))
        expected = CERTIFIED if mode == "carry" else REJECTED
        if verdict.kind != expected:
            ok = False
            print("  MISMATCH: expected %s" % expected)

    for kernel in (make_counter_kernel(), make_flush_kernel()):
        verdict = certify(kernel)
        print("%s: %s (%s)" % (verdict.kernel_name, verdict.kind, verdict.reason))
        if verdict.kind != OUTSIDE_FRAGMENT:
            ok = False
            print("  MISMATCH: expected OUTSIDE_FRAGMENT")

    probe = probe_exhaustive()
    print("probe: %s" % probe)
    if probe != EXPECTED_PROBE:
        ok = False
        print("MISMATCH: expected %s" % EXPECTED_PROBE)

    empty = probe_empty_callbacks()
    print("empty callbacks: %s" % empty)
    if empty != {"carry_preserves": True, "reset_violating_phases": [1, 2]}:
        ok = False
        print("MISMATCH: carry must preserve all phases; reset must violate 1 and 2")

    ramp = probe_ramp()
    print("ramp: %s" % ramp)
    if ramp != EXPECTED_RAMP:
        ok = False
        print("MISMATCH: expected %s" % EXPECTED_RAMP)

    print("ALL CHECKS PASSED" if ok else "FAILURES PRESENT")
    return 0 if ok else 1


if __name__ == "__main__":
    raise SystemExit(main())
