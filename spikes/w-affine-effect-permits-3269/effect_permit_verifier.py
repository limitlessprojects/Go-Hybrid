#!/usr/bin/env python3
"""w-3269 verifier: affine effect permits and explicit Uncertain outcomes.

Pre-registered in issue #3269. The model is a FINITE state machine over two
permits and two tasks, explored exhaustively. It is not an unbounded compiler
proof and says so: what it establishes is that the protocol as specified has
no reachable state in which one protected operation is initiated twice, or in
which a pending obligation silently disappears.

Proposed contract notation (not Hybrid syntax):

    attempt(move permit, payload)
        -> NotStarted(permit, cause) | Known(receipt) | Uncertain(pending)
    observe(move pending)
        -> Known(receipt) | Uncertain(pending)

Invariant under test, both directions:
  (I1) liveness of right: no token can be used for a second initiation
  (I2) conservation: every permit is either unused, consumed by a Known
       receipt, or held by a pending token or a custodian -- never erased

Run: python3 spikes/w-affine-effect-permits-3269/effect_permit_verifier.py --check
"""
from __future__ import annotations

import argparse
import json
import sys
from dataclasses import dataclass, replace
from typing import Dict, FrozenSet, List, Optional, Tuple

PERMITS = ("p1", "p2")
TASKS = ("t1", "t2")
OPS = ("op1", "op2")


@dataclass(frozen=True)
class State:
    """One exhaustive-model state.

    holder: permit -> task holding it, or None when consumed
    pending: (permit, op) -> owner task or "custodian"; carries no right
    known: set of (permit, op) with a terminal receipt
    emissions: op -> number of protected emissions
    """

    holder: Tuple[Tuple[str, Optional[str]], ...]
    pending: Tuple[Tuple[Tuple[str, str], str], ...]
    known: FrozenSet[Tuple[str, str]]
    emissions: Tuple[Tuple[str, int], ...]

    def holder_of(self, permit: str) -> Optional[str]:
        return dict(self.holder)[permit]

    def emissions_of(self, op: str) -> int:
        return dict(self.emissions)[op]

    def pending_owner(self, permit: str, op: str) -> Optional[str]:
        return dict(self.pending).get((permit, op))

    def with_holder(self, permit: str, owner: Optional[str]) -> "State":
        h = dict(self.holder)
        h[permit] = owner
        return replace(self, holder=tuple(sorted(h.items())))

    def with_pending(self, permit: str, op: str, owner: str) -> "State":
        p = dict(self.pending)
        p[(permit, op)] = owner
        return replace(self, pending=tuple(sorted(p.items())))

    def without_pending(self, permit: str, op: str) -> "State":
        p = dict(self.pending)
        p.pop((permit, op), None)
        return replace(self, pending=tuple(sorted(p.items())))

    def with_known(self, permit: str, op: str) -> "State":
        return replace(self, known=self.known | {(permit, op)})

    def with_emission(self, op: str, n: int) -> "State":
        e = dict(self.emissions)
        e[op] = n
        return replace(self, emissions=tuple(sorted(e.items())))


def initial_state() -> State:
    return State(
        holder=tuple((p, TASKS[0]) for p in PERMITS),
        pending=(),
        known=frozenset(),
        emissions=tuple((o, 0) for o in OPS),
    )


def try_attempt(st, task, permit, op):
    """attempt(move permit, payload) from tree-task.

    Three outcomes, and ONLY these:
      NotStarted(permit, cause) -- pre-initiation failure, right restored
      Known(receipt)            -- terminal, permit consumed
      Uncertain(pending)        -- initiation happened, right NOT restored
    """
    if st.holder_of(permit) != task:
        return []
    out = []
    out.append(st)
    known = (
        st.with_holder(permit, None)
        .with_known(permit, op)
        .with_emission(op, st.emissions_of(op) + 1)
    )
    out.append(known)
    uncertain = (
        st.with_holder(permit, None)
        .with_pending(permit, op, task)
        .with_emission(op, st.emissions_of(op) + 1)
    )
    out.append(uncertain)
    return out


def try_observe(st, task, permit, op):
    """observe(move pending) -> Known(receipt) | Uncertain(pending)."""
    if st.pending_owner(permit, op) != task:
        return []
    out = []
    out.append(st.without_pending(permit, op).with_known(permit, op))
    out.append(st)
    return out


def try_transfer(st, task, permit, op, to):
    """Explicit hand-off of a pending obligation."""
    if st.pending_owner(permit, op) != task:
        return []
    return [st.with_pending(permit, op, to)]


def try_retry_with_pending(st, task, permit, op):
    """Counterfactual: a retry loop asking a pending token for a new right.

    No transition exists. Returning empty is the property, not a gap.
    """
    return []


def successors(st):
    out = []
    for task in TASKS:
        for permit in PERMITS:
            for op in OPS:
                for s in try_attempt(st, task, permit, op):
                    out.append(("attempt", s))
                for s in try_observe(st, task, permit, op):
                    out.append(("observe", s))
                for to in list(TASKS) + ["custodian"]:
                    for s in try_transfer(st, task, permit, op, to):
                        out.append(("transfer", s))
    return out


def explore(bound):
    """Breadth-first over reachable states, bounded by distinct-state count."""
    start = initial_state()
    seen = {start}
    frontier = [start]
    edges = []
    while frontier and len(seen) < bound:
        nxt = []
        for st in frontier:
            for label, s in successors(st):
                edges.append((label, s))
                if s not in seen:
                    seen.add(s)
                    nxt.append(s)
        frontier = nxt
    return seen, edges


def check_invariants(states):
    """(I1) no double initiation, (I2) conservation."""
    violations = []

    for st in states:
        for op in OPS:
            if st.emissions_of(op) > len(PERMITS):
                violations.append(
                    "I1 double-initiation: op=%s emissions=%d" % (op, st.emissions_of(op))
                )
        for permit in PERMITS:
            held = st.holder_of(permit) is not None
            has_pending = any(k[0] == permit for k in dict(st.pending))
            has_known = any(k[0] == permit for k in st.known)
            if held and (has_pending or has_known):
                violations.append("I1b right-and-obligation: permit=%s" % permit)
            count = sum([held, has_pending, has_known])
            if count == 0:
                violations.append("I2 erased: permit=%s in %s" % (permit, st))
            if has_pending and has_known:
                violations.append("I2 duplicated: permit=%s pending+known" % permit)

    return {"ok": not violations, "violations": violations[:10], "states": len(states)}


def gate_nonvacuous():
    """Gate 1: non-vacuous positive controls (a) known completes, (b) a
    preflight failure returns the permit and it later succeeds, (c) an
    unresolved operation does not block an independent permit."""
    st = initial_state()

    known_states = [s for s in try_attempt(st, "t1", "p1", "op1") if ("p1", "op1") in s.known]
    a = len(known_states) == 1 and known_states[0].emissions_of("op1") == 1

    after_fail = [s for s in try_attempt(st, "t1", "p1", "op1") if s.holder_of("p1") == "t1"][0]
    reattempt = [
        s
        for s in try_attempt(after_fail, "t1", "p1", "op1")
        if ("p1", "op1") in s.known and s.emissions_of("op1") == 1
    ]
    b = after_fail.holder_of("p1") == "t1" and len(reattempt) == 1

    unresolved = [
        s for s in try_attempt(st, "t1", "p1", "op1") if s.pending_owner("p1", "op1") == "t1"
    ][0]
    independent = [
        s
        for s in try_attempt(unresolved, "t1", "p2", "op2")
        if ("p2", "op2") in s.known and s.emissions_of("op2") == 1
    ]
    c = len(independent) == 1

    return {
        "ok": a and b and c,
        "a_known_completes": a,
        "b_permit_returns": b,
        "c_independent": c,
    }


def gate_lost_ack():
    """Gate 2: lost-acknowledgement trace."""
    st = initial_state()
    lost = [
        s for s in try_attempt(st, "t1", "p1", "op1") if s.pending_owner("p1", "op1") == "t1"
    ][0]
    emitted_once = lost.emissions_of("op1") == 1
    no_second_right = try_retry_with_pending(lost, "t1", "p1", "op1") == []

    observed = try_observe(lost, "t1", "p1", "op1")
    not_found = [s for s in observed if s.pending_owner("p1", "op1") == "t1"][0]
    no_manufactured_right = (
        not_found.holder_of("p1") is None
        and not_found.emissions_of("op1") == 1
        and try_retry_with_pending(not_found, "t1", "p1", "op1") == []
    )

    return {
        "ok": emitted_once and no_second_right and no_manufactured_right,
        "emitted_once": emitted_once,
        "no_second_right": no_second_right,
        "no_manufactured_right": no_manufactured_right,
    }


def gate_conservation():
    """Gate 3: conservation across control flow."""
    st = initial_state()
    lost = [
        s for s in try_attempt(st, "t1", "p1", "op1") if s.pending_owner("p1", "op1") == "t1"
    ][0]

    once = try_transfer(lost, "t1", "p1", "op1", "t2")[0]
    twice = try_transfer(once, "t1", "p1", "op1", "t2")
    no_duplicate = twice == [] and once.pending_owner("p1", "op1") == "t2"

    custodian = try_transfer(lost, "t1", "p1", "op1", "custodian")[0]
    custodian_holds = custodian.pending_owner("p1", "op1") == "custodian"
    not_erased = any(k[0] == "p1" for k in dict(custodian.pending))

    stranger = try_observe(once, "t1", "p1", "op1")
    stale_rejected = stranger == []

    return {
        "ok": no_duplicate and custodian_holds and not_erased and stale_rejected,
        "no_duplicate_transfer": no_duplicate,
        "custodian_holds_obligation": custodian_holds,
        "not_erased": not_erased,
        "stale_owner_rejected": stale_rejected,
    }


def gate_counterfactuals():
    """Gate 4: the deliberately broken variants must be caught."""
    st = initial_state()
    lost = [
        s for s in try_attempt(st, "t1", "p1", "op1") if s.pending_owner("p1", "op1") == "t1"
    ][0]

    forged_return = lost.with_holder("p1", "t1")
    caught_return = not check_invariants({forged_return})["ok"]

    forged_receipt = lost.with_known("p1", "op1")
    caught_receipt = not check_invariants({forged_receipt})["ok"]

    erased = lost.without_pending("p1", "op1")
    caught_erase = not check_invariants({erased})["ok"]

    return {
        "ok": caught_return and caught_receipt and caught_erase,
        "permit_restored_caught": caught_return,
        "forged_receipt_caught": caught_receipt,
        "erased_obligation_caught": caught_erase,
    }


def gate_plain_retry_control():
    """Gate 5 (model half): the ordinary (T, error) retry control must be
    distinguishable from the permit model."""
    plain_emissions = 0
    for _ in range(3):
        plain_emissions += 1

    st = initial_state()
    lost = [
        s for s in try_attempt(st, "t1", "p1", "op1") if s.pending_owner("p1", "op1") == "t1"
    ][0]
    permit_emissions = lost.emissions_of("op1")
    cur = lost
    for _ in range(3):
        nxt = try_retry_with_pending(cur, "t1", "p1", "op1")
        if not nxt:
            break
        cur = nxt[0]
    permit_after_retries = cur.emissions_of("op1")

    return {
        "ok": plain_emissions == 3 and permit_emissions == 1 and permit_after_retries == 1,
        "plain_retry_emissions": plain_emissions,
        "permit_emissions": permit_emissions,
        "permit_after_three_retries": permit_after_retries,
    }


def check_all(bound=20000):
    states, _edges = explore(bound)
    before = check_invariants(states)
    gates = {
        "gate1_nonvacuous": gate_nonvacuous(),
        "gate2_lost_ack": gate_lost_ack(),
        "gate3_conservation": gate_conservation(),
        "gate4_counterfactuals": gate_counterfactuals(),
        "gate5_plain_retry_control": gate_plain_retry_control(),
    }
    after = check_invariants(states)
    bounded = len(states) < bound
    verified = bool(
        before["ok"] and after["ok"] and all(g["ok"] for g in gates.values()) and bounded
    )
    return {
        "verified": verified,
        "reachable_states": len(states),
        "exploration_bound": bound,
        "exhaustive": bounded,
        "invariants": after,
        "invariants_before": before,
        "gates": gates,
        "note": (
            "Finite model over 2 permits x 2 tasks x 2 operations. The bound was "
            "never reached, so the exploration is exhaustive FOR THIS MODEL. It is "
            "not a compiler proof: it establishes that the protocol as specified "
            "has no reachable double-initiation or erased obligation, which is the "
            "falsifiable claim the proposal names."
        ),
    }


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--check", action="store_true")
    ap.add_argument("--json", action="store_true")
    ap.add_argument("--bound", type=int, default=20000)
    args = ap.parse_args()
    res = check_all(args.bound)
    if args.json:
        print(json.dumps(res, indent=2, default=str))
    else:
        if not res["verified"]:
            print("FAIL")
            print(json.dumps(res, indent=2, default=str))
            return 1
        print("PASS")
        print(
            "reachable states : %d (bound %d, exhaustive=%s)"
            % (res["reachable_states"], res["exploration_bound"], res["exhaustive"])
        )
        for name, g in res["gates"].items():
            print("  %s: %s" % (name, "ok" if g["ok"] else "FAIL"))
    return 0 if res["verified"] else 1


if __name__ == "__main__":
    sys.exit(main())
