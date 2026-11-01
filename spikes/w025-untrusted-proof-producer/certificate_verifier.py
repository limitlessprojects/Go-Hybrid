#!/usr/bin/env python3
"""
Certificate Verifier Kernel for W-025 (Go-Hybrid Untrusted Proof Producer)
Strict deterministic rule-checker: < 300 LOC, zero global search / SMT solver.
"""

from typing import Dict, List, Set, Tuple, Optional

class VerifierError(Exception):
    def __init__(self, code: str, message: str):
        self.code = code
        self.message = message
        super().__init__(f"[{code}] {message}")


class CertificateVerifier:
    def __init__(self):
        self.roots: Dict[str, int] = {}  # root_id -> length
        self.views: Dict[str, Tuple[str, int, int]] = {}  # view_id -> (root_id, start, end)
        self.task_views: Dict[str, Set[str]] = {}  # task_id -> set of active view_ids
        self.transferred_roots: Dict[str, str] = {}  # root_id -> recipient_task_id

    def register_alloc(self, root_id: str, length: int):
        if root_id in self.roots:
            raise VerifierError("ERR_FORGED_CERTIFICATE_INTEGRITY", f"Duplicate allocation root: {root_id}")
        self.roots[root_id] = length

    def register_slice(self, view_id: str, root_id: str, start: int, end: int):
        if root_id not in self.roots:
            raise VerifierError("ERR_FORGED_CERTIFICATE_INTEGRITY", f"Slice points to undefined root: {root_id}")
        root_len = self.roots[root_id]
        if start < 0 or end > root_len or start > end:
            raise VerifierError("ERR_FORGED_CERTIFICATE_INTEGRITY", f"Slice range [{start}:{end}] out of bounds for root length {root_len}")
        self.views[view_id] = (root_id, start, end)

    def bind_view_to_task(self, task_id: str, view_id: str):
        if view_id not in self.views:
            raise VerifierError("ERR_FORGED_CERTIFICATE_INTEGRITY", f"Unknown view: {view_id}")
        if task_id not in self.task_views:
            self.task_views[task_id] = set()
        self.task_views[task_id].add(view_id)

    def check_transfer(self, from_task: str, to_task: str, transferred_view_id: str):
        """Verifies that transferring transferred_view_id to to_task does not leave active overlapping views in from_task."""
        if transferred_view_id not in self.views:
            raise VerifierError("ERR_FORGED_CERTIFICATE_INTEGRITY", f"Unknown transferred view: {transferred_view_id}")

        root_id, t_start, t_end = self.views[transferred_view_id]
        
        # Check all other views currently retained by from_task
        retained = self.task_views.get(from_task, set())
        for other_id in list(retained):
            if other_id == transferred_view_id:
                continue
            o_root, o_start, o_end = self.views[other_id]
            if o_root == root_id:
                # Check interval overlap
                if max(t_start, o_start) < min(t_end, o_end):
                    raise VerifierError(
                        "ERR_SURVIVING_ALIAS_AFTER_TRANSFER",
                        f"Task '{from_task}' retains view '{other_id}' [{o_start}:{o_end}] overlapping with transferred view '{transferred_view_id}' [{t_start}:{t_end}] on root '{root_id}'"
                    )

        # Remove transferred view from from_task and assign to to_task
        retained.discard(transferred_view_id)
        if to_task not in self.task_views:
            self.task_views[to_task] = set()
        self.task_views[to_task].add(transferred_view_id)
        self.transferred_roots[root_id] = to_task

    def check_concurrent_access(self, task_a: str, view_a: str, is_write_a: bool,
                                task_b: str, view_b: str, is_write_b: bool):
        """Checks whether two tasks concurrently accessing view_a and view_b violate race-freedom."""
        if not (is_write_a or is_write_b):
            return  # Concurrent reads are sound

        root_a, s_a, e_a = self.views[view_a]
        root_b, s_b, e_b = self.views[view_b]

        if root_a != root_b:
            return  # Disjoint roots cannot race

        # Overlap check
        overlap_start = max(s_a, s_b)
        overlap_end = min(e_a, e_b)
        if overlap_start < overlap_end:
            raise VerifierError(
                "ERR_OVERLAPPING_CONCURRENT_WRITE",
                f"Concurrent mutation detected: task '{task_a}' (view '{view_a}' [{s_a}:{e_a}]) and task '{task_b}' (view '{view_b}' [{s_b}:{e_b}]) overlap on bytes [{overlap_start}:{overlap_end}]"
            )

    def verify_program_trace(self, trace: dict) -> str:
        """Parses and validates a proof derivation certificate against CFG events."""
        # 1. Register allocations
        for alloc in trace.get("allocations", []):
            self.register_alloc(alloc["root_id"], alloc["length"])

        # 2. Register slice projections
        for s in trace.get("slices", []):
            self.register_slice(s["view_id"], s["root_id"], s["start"], s["end"])

        # 3. Initial task assignments
        for binding in trace.get("initial_bindings", []):
            self.bind_view_to_task(binding["task_id"], binding["view_id"])

        # 4. Process event sequence
        for ev in trace.get("events", []):
            ev_type = ev.get("type")
            if ev_type == "transfer":
                self.check_transfer(ev["from_task"], ev["to_task"], ev["view_id"])
            elif ev_type == "concurrent_access":
                self.check_concurrent_access(
                    ev["task_a"], ev["view_a"], ev["write_a"],
                    ev["task_b"], ev["view_b"], ev["write_b"]
                )
            elif ev_type == "access":
                # Ensure task holding the view hasn't ceded it
                task = ev["task"]
                v_id = ev["view_id"]
                if v_id not in self.task_views.get(task, set()):
                    raise VerifierError("ERR_USE_AFTER_TRANSFER", f"Task '{task}' attempted access on ceded view '{v_id}'")
            else:
                raise VerifierError("ERR_FORGED_CERTIFICATE_INTEGRITY", f"Unknown event type: {ev_type}")

        return "CERT_VERIFIED"
