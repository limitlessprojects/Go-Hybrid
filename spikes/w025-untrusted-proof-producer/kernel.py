#!/usr/bin/env python3
"""
Independent Permission Verification Kernel for Go-Hybrid (Wildcard W-025).
Zero SMT / Zero Global Search. Direct deterministic CFG & memory operation verifier.
"""

from typing import Dict, List, Set, Tuple, Optional, Any


class VerifierError(Exception):
    def __init__(self, code: str, message: str):
        self.code = code
        self.message = message
        super().__init__(f"[{code}] {message}")


class ProofVerifierKernel:
    """
    Independent Kernel verifying untrusted candidate derivations against actual Go CFG and memory operations.
    Budget constraint: < 300 non-generated semantic LOC.
    """

    def __init__(self):
        self.roots: Dict[str, int] = {}  # root_id -> length
        self.views: Dict[str, Tuple[str, int, int]] = {}  # view_id -> (root_id, start, end)
        self.task_views: Dict[str, Set[str]] = {}  # task_id -> set of owned view_ids
        self.fresh_roots: Set[str] = set()

    def register_alloc(self, root_id: str, length: int, is_fresh: bool = True):
        if root_id in self.roots:
            raise VerifierError("ERR_FORGED_CERTIFICATE_INTEGRITY", f"Duplicate allocation root: {root_id}")
        if length <= 0:
            raise VerifierError("ERR_FORGED_CERTIFICATE_INTEGRITY", f"Invalid allocation size: {length}")
        self.roots[root_id] = length
        if is_fresh:
            self.fresh_roots.add(root_id)

    def register_slice(self, view_id: str, root_id: str, start: int, end: int):
        if root_id not in self.roots:
            raise VerifierError("ERR_FORGED_CERTIFICATE_INTEGRITY", f"View '{view_id}' targets undefined root '{root_id}'")
        root_len = self.roots[root_id]
        if start < 0 or end > root_len or start > end:
            raise VerifierError("ERR_FORGED_CERTIFICATE_INTEGRITY", f"Slice range [{start}:{end}] out-of-bounds for root '{root_id}' (len {root_len})")
        self.views[view_id] = (root_id, start, end)

    def bind_initial_view(self, task_id: str, view_id: str):
        if view_id not in self.views:
            raise VerifierError("ERR_FORGED_CERTIFICATE_INTEGRITY", f"Unknown view: {view_id}")
        self.task_views.setdefault(task_id, set()).add(view_id)

    def verify_ownership_transfer(self, from_task: str, to_task: str, view_id: str):
        """
        Enforces that transferring view_id to to_task cedes all overlapping views in from_task.
        If from_task retains any alias overlapping with view_id, reject with ERR_SURVIVING_ALIAS_AFTER_TRANSFER.
        """
        if view_id not in self.views:
            raise VerifierError("ERR_FORGED_CERTIFICATE_INTEGRITY", f"Unknown view in transfer: {view_id}")
        
        from_views = self.task_views.get(from_task, set())
        if view_id not in from_views:
            raise VerifierError("ERR_USE_AFTER_TRANSFER", f"Task '{from_task}' does not hold view '{view_id}' to transfer")

        root_id, t_start, t_end = self.views[view_id]

        # Check for surviving overlapping aliases retained by from_task
        for other_id in from_views:
            if other_id == view_id:
                continue
            o_root, o_start, o_end = self.views[other_id]
            if o_root == root_id:
                # Interval intersection check
                if max(t_start, o_start) < min(t_end, o_end):
                    raise VerifierError(
                        "ERR_SURVIVING_ALIAS_AFTER_TRANSFER",
                        f"Task '{from_task}' retains view '{other_id}' [{o_start}:{o_end}] overlapping with transferred '{view_id}' [{t_start}:{t_end}] on root '{root_id}'"
                    )

        # Cede view
        from_views.remove(view_id)
        self.task_views.setdefault(to_task, set()).add(view_id)

    def verify_concurrent_mutation(self, task_a: str, view_a: str, is_write_a: bool,
                                   task_b: str, view_b: str, is_write_b: bool):
        """
        Verifies that concurrent operations on view_a and view_b do not overlap if either is a write.
        """
        if not (is_write_a or is_write_b):
            return  # Concurrent reads are sound

        root_a, s_a, e_a = self.views[view_a]
        root_b, s_b, e_b = self.views[view_b]

        if root_a != root_b:
            return  # Different roots cannot race

        overlap_start = max(s_a, s_b)
        overlap_end = min(e_a, e_b)
        if overlap_start < overlap_end:
            raise VerifierError(
                "ERR_OVERLAPPING_CONCURRENT_WRITE",
                f"Concurrent mutation conflict on root '{root_a}' [{overlap_start}:{overlap_end}] between task '{task_a}' (view '{view_a}') and task '{task_b}' (view '{view_b}')"
            )

    def verify_proof(self, proof: Dict[str, Any], actual_cfg: List[Dict[str, Any]], actual_memory_ops: List[Dict[str, Any]]) -> str:
        """
        Main entry point: Validates proof derivation against actual typed Go CFG and memory operations.
        """
        # Step 1: Verify CFG coverage
        proof_edges = {(e["from"], e["to"], e.get("type", "step")) for e in proof.get("cfg_edges", [])}
        actual_edges = {(e["from"], e["to"], e.get("type", "step")) for e in actual_cfg}
        if not actual_edges.issubset(proof_edges):
            missing = actual_edges - proof_edges
            raise VerifierError("ERR_CFG_COVERAGE_MISMATCH", f"Proof omits CFG edges present in actual program: {missing}")

        # Step 2: Register allocations and slices
        for alloc in proof.get("allocations", []):
            self.register_alloc(alloc["root_id"], alloc["length"], alloc.get("is_fresh", True))

        for sl in proof.get("slices", []):
            self.register_slice(sl["view_id"], sl["root_id"], sl["start"], sl["end"])

        for b in proof.get("initial_bindings", []):
            self.bind_initial_view(b["task_id"], b["view_id"])

        # Step 3: Verify execution trace events
        for ev in proof.get("events", []):
            ev_type = ev["type"]
            if ev_type == "transfer":
                self.verify_ownership_transfer(ev["from_task"], ev["to_task"], ev["view_id"])
            elif ev_type == "concurrent_access":
                self.verify_concurrent_mutation(
                    ev["task_a"], ev["view_a"], ev.get("write_a", False),
                    ev["task_b"], ev["view_b"], ev.get("write_b", False)
                )
            elif ev_type == "access":
                task = ev["task"]
                v_id = ev["view_id"]
                if v_id not in self.task_views.get(task, set()):
                    raise VerifierError("ERR_USE_AFTER_TRANSFER", f"Task '{task}' attempted access on ceded view '{v_id}'")
            else:
                raise VerifierError("ERR_FORGED_CERTIFICATE_INTEGRITY", f"Unknown derivation event type: {ev_type}")

        # Step 4: Verify memory operations match actual program
        proof_ops = {(op["addr"], op["type"], op["task"]) for op in proof.get("memory_ops", [])}
        for act_op in actual_memory_ops:
            op_key = (act_op["addr"], act_op["type"], act_op["task"])
            if op_key not in proof_ops:
                raise VerifierError("ERR_MEMORY_OP_MISMATCH", f"Actual memory operation not accounted for in proof: {act_op}")

        return "CERT_VERIFIED"
