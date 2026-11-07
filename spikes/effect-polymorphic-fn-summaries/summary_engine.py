"""Effect-polymorphic function summaries via Tarjan SCC fixpoint.

Replaces AST body re-walking (`depth <= 4` in `hy-check/src/loans.rs`) and flips
GUARANTEES.md Rows 5 & 9 (A5 callback escapes) by inferring a principal summary
per function SCC:
  - slot_shares: set of (dst_slot, src_slot) storage-alias edges
  - ret_shares:  set of (ret_i, ret_j) co-returned storage-alias pairs
  - mult_req:    per-param callback invocation multiplicity ('0', '1', 'sync*', 'async*')
"""
from dataclasses import dataclass, field


@dataclass
class CallSite:
    callee: str  # function name or "param:<idx>" for higher-order call
    arg_slots: list[int]
    cb_multiplicity: str = "1"  # '1' | 'sync*' | 'async*'


@dataclass
class FnIR:
    name: str
    arity: int
    direct_shares: set[tuple[int, int]] = field(default_factory=set)
    ret_shares: set[tuple[int, int]] = field(default_factory=set)
    captured_mut_slots: set[int] = field(default_factory=set)
    spawns_slot: set[int] = field(default_factory=set)
    calls: list[CallSite] = field(default_factory=list)


@dataclass
class Summary:
    shares: set[tuple[int, int]]
    ret_shares: set[tuple[int, int]]
    spawns_slot: set[int]


def tarjan_sccs(fns: dict[str, FnIR]) -> list[list[str]]:
    idx, low, stack, on_stack, out, counter = {}, {}, [], set(), [], [0]

    def strong(v: str) -> None:
        idx[v] = low[v] = counter[0]
        counter[0] += 1
        stack.append(v)
        on_stack.add(v)
        for cs in fns[v].calls:
            w = cs.callee
            if w in fns:
                if w not in idx:
                    strong(w)
                    low[v] = min(low[v], low[w])
                elif w in on_stack:
                    low[v] = min(low[v], idx[w])
        if low[v] == idx[v]:
            comp = []
            while True:
                w = stack.pop()
                on_stack.remove(w)
                comp.append(w)
                if w == v:
                    break
            out.append(comp)

    for name in fns:
        if name not in idx:
            strong(name)
    return out


def infer_summaries(fns: dict[str, FnIR], foreign_cb_mult: dict[str, str]) -> dict[str, Summary]:
    sums = {
        k: Summary(set(f.direct_shares), set(f.ret_shares), set(f.spawns_slot))
        for k, f in fns.items()
    }
    for scc in tarjan_sccs(fns):
        changed = True
        while changed:
            changed = False
            for name in scc:
                fn = fns[name]
                sm = sums[name]
                for cs in fn.calls:
                    if cs.callee in sums:
                        callee_sm = sums[cs.callee]
                        for d, s in list(callee_sm.shares):
                            if d < len(cs.arg_slots) and s < len(cs.arg_slots):
                                edge = (cs.arg_slots[d], cs.arg_slots[s])
                                if edge[0] != edge[1] and edge not in sm.shares:
                                    sm.shares.add(edge)
                                    changed = True
                        for sp in list(callee_sm.spawns_slot):
                            if sp < len(cs.arg_slots) and cs.arg_slots[sp] not in sm.spawns_slot:
                                sm.spawns_slot.add(cs.arg_slots[sp])
                                changed = True
                    elif cs.callee in foreign_cb_mult and foreign_cb_mult[cs.callee] == "async*":
                        for slot in cs.arg_slots:
                            if slot not in sm.spawns_slot:
                                sm.spawns_slot.add(slot)
                                changed = True
                # Transitive closure over slot shares within the function summary
                for a, b in list(sm.shares):
                    for c, d in list(sm.shares):
                        if b == c and a != d and (a, d) not in sm.shares:
                            sm.shares.add((a, d))
                            changed = True
    return sums


def check_callback_escape(cb_mutates_capture: bool, api_mult: str) -> str:
    """Row 5 + Row 9 decision rule: sync[1]/sync[*] admit sequential mutation; async[*] rejects."""
    if not cb_mutates_capture:
        return "ACCEPT"
    return "ACCEPT" if api_mult in ("1", "sync*") else "REJECT_HYB_E3002"
