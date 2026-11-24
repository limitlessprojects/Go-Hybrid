#!/usr/bin/env python3
"""
Formal Model and State Space Verifier for Ownership-Breaking Client Synthesis.
Answering Proposal #4729: "[wildcard] Make an ownership-breaking release produce its smallest downstream client"

Exhaustively verifies:
1. Enumeration of all 127 sequences of length 0..6 across {Submit, Inspect}.
2. Proof that for borrow -> consume, exactly 114 sequences pass in Old and fail in New.
3. Shortest witness is length 2: ['Submit', 'Inspect'] (or ['Submit', 'Submit']).
4. Proof that for consume -> borrow (relaxation), exactly 0 breaking sequences exist.
5. Proof that for unchanged contracts, exactly 0 breaking sequences exist.
"""

from collections import deque
from enum import Enum
from typing import List, Tuple, Dict, Optional

class Effect(Enum):
    PRESERVES = "PRESERVES"
    CONSUMES = "CONSUMES"
    PRODUCES = "PRODUCES"
    UNSUPPORTED = "UNSUPPORTED"

class State(Enum):
    LIVE = "LIVE"
    CONSUMED = "CONSUMED"
    POISONED = "POISONED"

def apply_op(effect: Effect, state: State) -> Tuple[State, bool, str]:
    if effect == Effect.PRESERVES:
        if state == State.CONSUMED:
            return State.POISONED, False, "HYB-E3003"
        return State.LIVE, True, ""
    elif effect == Effect.CONSUMES:
        if state == State.CONSUMED:
            return State.POISONED, False, "HYB-E3003"
        return State.CONSUMED, True, ""
    elif effect == Effect.PRODUCES:
        return State.LIVE, True, ""
    elif effect == Effect.UNSUPPORTED:
        return State.POISONED, False, "UNKNOWN"
    return State.POISONED, False, "INTERNAL_ERROR"

def run_sequence(seq: List[str], contract: Dict[str, Effect], init_state: State = State.LIVE) -> Tuple[bool, Optional[int], str]:
    cur = init_state
    for i, op in enumerate(seq):
        eff = contract.get(op, Effect.UNSUPPORTED)
        nxt, ok, diag = apply_op(eff, cur)
        if not ok:
            return False, i, diag
        cur = nxt
    return True, None, ""

def enumerate_all_sequences(ops: List[str], max_len: int) -> List[List[str]]:
    results = [[]]
    cur_level = [[]]
    for _ in range(max_len):
        next_level = []
        for path in cur_level:
            for op in ops:
                nxt = path + [op]
                results.append(nxt)
                next_level.append(nxt)
        cur_level = next_level
    return results

def find_shortest_witness(old_contract: Dict[str, Effect], new_contract: Dict[str, Effect], ops: List[str], max_len: int = 6) -> Optional[Tuple[List[str], int, str]]:
    queue = deque([([], State.LIVE, State.LIVE)])
    while queue:
        path, old_st, new_st = queue.popleft()
        if len(path) >= max_len:
            continue
        for op in ops:
            nxt_path = path + [op]
            old_nxt, old_ok, _ = apply_op(old_contract[op], old_st)
            new_nxt, new_ok, new_diag = apply_op(new_contract[op], new_st)

            if old_ok:
                if not new_ok:
                    return nxt_path, len(nxt_path) - 1, new_diag
                queue.append((nxt_path, old_nxt, new_nxt))
    return None

def verify_full_census():
    ops = ["Submit", "Inspect"]
    old_contract_borrow = {"Submit": Effect.PRESERVES, "Inspect": Effect.PRESERVES}
    new_contract_consume = {"Submit": Effect.CONSUMES, "Inspect": Effect.PRESERVES}

    all_seqs = enumerate_all_sequences(ops, max_len=6)
    total_seqs = len(all_seqs) # 127
    assert total_seqs == 127, f"Expected 127 total sequences, got {total_seqs}"

    # 1. Test borrow -> consume
    breaking_count = 0
    shortest_witness = None

    for seq in all_seqs:
        old_ok, _, _ = run_sequence(seq, old_contract_borrow)
        new_ok, fail_idx, diag = run_sequence(seq, new_contract_consume)
        if old_ok and not new_ok:
            breaking_count += 1
            if shortest_witness is None:
                shortest_witness = (seq, fail_idx, diag)

    print(f"Total sequences (depth 0..6): {total_seqs}")
    print(f"Borrow -> Consume: Breaking witnesses = {breaking_count} / {total_seqs}")
    assert breaking_count == 114, f"Expected 114 breaking sequences, got {breaking_count}"

    # 2. Test reverse: consume -> borrow
    reverse_breaking_count = 0
    for seq in all_seqs:
        old_ok, _, _ = run_sequence(seq, new_contract_consume)
        new_ok, _, _ = run_sequence(seq, old_contract_borrow)
        if old_ok and not new_ok:
            reverse_breaking_count += 1

    print(f"Consume -> Borrow (Relaxation): Breaking witnesses = {reverse_breaking_count} / {total_seqs}")
    assert reverse_breaking_count == 0, f"Expected 0 breaking sequences for relaxation, got {reverse_breaking_count}"

    # 3. Test unchanged contract
    unchanged_breaking_count = 0
    for seq in all_seqs:
        old_ok, _, _ = run_sequence(seq, old_contract_borrow)
        new_ok, _, _ = run_sequence(seq, old_contract_borrow)
        if old_ok and not new_ok:
            unchanged_breaking_count += 1

    print(f"Unchanged: Breaking witnesses = {unchanged_breaking_count} / {total_seqs}")
    assert unchanged_breaking_count == 0, f"Expected 0 breaking sequences for unchanged contract, got {unchanged_breaking_count}"

    # 4. Shortest witness check
    # With Inspect visited first, shortest witness is ['Submit', 'Inspect']
    sw = find_shortest_witness(old_contract_borrow, new_contract_consume, ops=["Inspect", "Submit"])
    assert sw is not None
    seq, idx, diag = sw
    print(f"Shortest witness with Inspect first: {seq} (fails at index {idx} with {diag})")
    assert seq == ["Submit", "Inspect"], f"Expected ['Submit', 'Inspect'], got {seq}"
    assert diag == "HYB-E3003", f"Expected HYB-E3003, got {diag}"

    print("ALL STATE SPACE AXIOMS VERIFIED SUCCESSFULLY.")

if __name__ == "__main__":
    verify_full_census()
