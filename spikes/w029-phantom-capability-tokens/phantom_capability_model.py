#!/usr/bin/env python3
"""
phantom_capability_model.py — Standalone Model & Type Constraint Verifier
Moonshot Track Proposal W-029: Zero-Alias Phantom Capability Tokens

Implements:
1. Compile-Time Linear Capability Lattice (Unique -> Shared -> Expired).
2. Lexical Fence Receptor to eliminate closure capture storage sharing (W-004 Falsifier fix).
3. Graph Complexity Analyzer: compares AST Path-Sensitive Alias-Graph Walk (O(V * E))
   versus Phantom Capability Type Checking (O(N)).
4. 20-Case Canonical Conformance Suite (10 Positive, 10 Negative).
"""

import time
import copy
from typing import Dict, List, Set, Tuple, Optional, Any

class Permission:
    UNIQUE = "Unique"
    SHARED = "Shared"
    EXPIRED = "Expired"

class Capability:
    def __init__(self, type_name: str, perm: str, value: Any = None):
        self.type_name = type_name
        self.perm = perm
        self.value = value
        self.version = 0

    def __repr__(self):
        return f"Cap[{self.type_name}, {self.perm}](v={self.version})"

class Diagnostic:
    def __init__(self, code: str, message: str, span: str):
        self.code = code
        self.message = message
        self.span = span

    def __repr__(self):
        return f"error[{self.code}]: {self.message} ({self.span})"

class PhantomTypeChecker:
    """
    Lightweight, zero-alias-graph type checker that checks capability transitions
    as localized type-system constraint checks rather than path-sensitive graph walks.
    """
    def __init__(self):
        self.bindings: Dict[str, Capability] = {}
        self.diagnostics: List[Diagnostic] = []
        self.active_borrows: Dict[str, int] = {}
        self.task_spawns: List[Dict[str, Any]] = []

    def declare_unique(self, var_name: str, type_name: str, value: Any = None) -> Capability:
        cap = Capability(type_name, Permission.UNIQUE, value)
        self.bindings[var_name] = cap
        return cap

    def read_shared(self, var_name: str, span: str = "inline:1") -> Optional[Any]:
        cap = self.bindings.get(var_name)
        if not cap:
            self.diagnostics.append(Diagnostic("HYB-E1001", f"variable `{var_name}` not in scope", span))
            return None
        if cap.perm == Permission.EXPIRED:
            self.diagnostics.append(Diagnostic("HYB-E3003", f"use of moved/expired capability `{var_name}`", span))
            return None
        return cap.value

    def mutate_unique(self, var_name: str, mutator_fn, span: str = "inline:1") -> bool:
        cap = self.bindings.get(var_name)
        if not cap:
            self.diagnostics.append(Diagnostic("HYB-E1001", f"variable `{var_name}` not in scope", span))
            return False
        if cap.perm == Permission.EXPIRED:
            self.diagnostics.append(Diagnostic("HYB-E3003", f"use of moved/expired capability `{var_name}`", span))
            return False
        if cap.perm == Permission.SHARED:
            self.diagnostics.append(Diagnostic("HYB-E3006", f"cannot mutate through shared borrow `{var_name}`", span))
            return False
        if self.active_borrows.get(var_name, 0) > 0:
            self.diagnostics.append(Diagnostic("HYB-E3004", f"cannot mutate `{var_name}` while active shared borrows exist", span))
            return False

        cap.value = mutator_fn(cap.value)
        cap.version += 1
        return True

    def scoped_borrow(self, var_name: str, borrow_block_fn, span: str = "inline:1") -> Any:
        cap = self.bindings.get(var_name)
        if not cap or cap.perm != Permission.UNIQUE:
            self.diagnostics.append(Diagnostic("HYB-E3003", f"cannot borrow `{var_name}`: requires Unique capability", span))
            return None
        
        self.active_borrows[var_name] = self.active_borrows.get(var_name, 0) + 1
        borrowed_cap = Capability(cap.type_name, Permission.SHARED, cap.value)
        try:
            res = borrow_block_fn(borrowed_cap)
            return res
        finally:
            self.active_borrows[var_name] -= 1

    def transfer_unique(self, src_var: str, dst_var: str, span: str = "inline:1") -> bool:
        cap = self.bindings.get(src_var)
        if not cap:
            self.diagnostics.append(Diagnostic("HYB-E1001", f"variable `{src_var}` not in scope", span))
            return False
        if cap.perm == Permission.EXPIRED:
            self.diagnostics.append(Diagnostic("HYB-E3003", f"use of moved/expired capability `{src_var}`", span))
            return False
        if cap.perm == Permission.SHARED:
            self.diagnostics.append(Diagnostic("HYB-E3002", f"cannot transfer shared capability `{src_var}` as unique", span))
            return False
        if self.active_borrows.get(src_var, 0) > 0:
            self.diagnostics.append(Diagnostic("HYB-E3004", f"cannot move `{src_var}` while active shared borrows exist", span))
            return False

        cap.perm = Permission.EXPIRED
        new_cap = Capability(cap.type_name, Permission.UNIQUE, cap.value)
        self.bindings[dst_var] = new_cap
        return True

    def spawn_task(self, task_name: str, captured_vars: List[str], span: str = "inline:1") -> bool:
        """
        Enforces task boundary lexical fence.
        Resolves W-004 Falsifier: any uniquely captured variable must be affine-moved
        and invalidated in caller's environment, preventing shared closure storage.
        """
        for var in captured_vars:
            cap = self.bindings.get(var)
            if not cap:
                self.diagnostics.append(Diagnostic("HYB-E1001", f"variable `{var}` not declared", span))
                return False
            if cap.perm == Permission.EXPIRED:
                self.diagnostics.append(Diagnostic("HYB-E3003", f"cannot capture moved/expired capability `{var}` in task `{task_name}`", span))
                return False
            if self.active_borrows.get(var, 0) > 0:
                self.diagnostics.append(Diagnostic("HYB-E3001", f"cannot send `{var}` across task boundary while borrowed", span))
                return False
            
            # Affine fence discharges caller's access
            cap.perm = Permission.EXPIRED

        self.task_spawns.append({"task": task_name, "captured": captured_vars})
        return True

    def split_pair(self, pair_var: str, fst_var: str, snd_var: str, span: str = "inline:1") -> bool:
        cap = self.bindings.get(pair_var)
        if not cap or cap.perm != Permission.UNIQUE:
            self.diagnostics.append(Diagnostic("HYB-E3003", f"split requires Unique pair `{pair_var}`", span))
            return False
        cap.perm = Permission.EXPIRED
        val_first, val_second = cap.value if isinstance(cap.value, (tuple, list)) else (None, None)
        self.bindings[fst_var] = Capability(f"{cap.type_name}.Fst", Permission.UNIQUE, val_first)
        self.bindings[snd_var] = Capability(f"{cap.type_name}.Snd", Permission.UNIQUE, val_second)
        return True

def run_test_matrix() -> List[Dict[str, Any]]:
    results = []

    # --- 10 POSITIVE CONFORMANCE CASES ---
    # P01: Unique Linear Transfer
    tc = PhantomTypeChecker()
    tc.declare_unique("x", "Buffer", [1, 2, 3])
    p01_ok = tc.transfer_unique("x", "y") and len(tc.diagnostics) == 0
    results.append({"id": "P01", "name": "Unique Linear Transfer", "passed": p01_ok, "kind": "POSITIVE"})

    # P02: Scoped Read Borrowing
    tc = PhantomTypeChecker()
    tc.declare_unique("buf", "Buffer", "hello")
    borrowed_val = tc.scoped_borrow("buf", lambda s: tc.read_shared("buf"))
    p02_ok = borrowed_val == "hello" and len(tc.diagnostics) == 0
    results.append({"id": "P02", "name": "Scoped Read Borrowing", "passed": p02_ok, "kind": "POSITIVE"})

    # P03: Concurrent Independent Field Mutex
    tc = PhantomTypeChecker()
    tc.declare_unique("pair", "Pair", ("dataA", "dataB"))
    tc.split_pair("pair", "partA", "partB")
    m_ok = tc.mutate_unique("partA", lambda v: v + "_mod") and tc.mutate_unique("partB", lambda v: v + "_mod")
    results.append({"id": "P03", "name": "Disjoint Field Split & Mutation", "passed": m_ok and len(tc.diagnostics) == 0, "kind": "POSITIVE"})

    # P04: Pipeline Stream Processing
    tc = PhantomTypeChecker()
    tc.declare_unique("s0", "Stream", [10, 20])
    tc.transfer_unique("s0", "s1")
    tc.transfer_unique("s1", "s2")
    results.append({"id": "P04", "name": "Pipeline Stream Linear Propagation", "passed": len(tc.diagnostics) == 0, "kind": "POSITIVE"})

    # P05: Task Spawn with Affine Fence (W-004 fix)
    tc = PhantomTypeChecker()
    tc.declare_unique("job", "JobData", {"id": 42})
    spawn_ok = tc.spawn_task("WorkerTask", ["job"])
    results.append({"id": "P05", "name": "Task Spawn with Affine Fence", "passed": spawn_ok and len(tc.diagnostics) == 0, "kind": "POSITIVE"})

    # P06: Multi-Stage Map-Reduce
    tc = PhantomTypeChecker()
    tc.declare_unique("dataset", "Matrix", [1, 2, 3, 4])
    tc.scoped_borrow("dataset", lambda d: sum(d.value))
    tc.mutate_unique("dataset", lambda v: [x * 2 for x in v])
    results.append({"id": "P06", "name": "Sequential Borrow-then-Mutate", "passed": len(tc.diagnostics) == 0, "kind": "POSITIVE"})

    # P07: Nested Re-entrant Borrows
    tc = PhantomTypeChecker()
    tc.declare_unique("node", "Tree", {"left": 1, "right": 2})
    def outer(b1):
        return tc.scoped_borrow("node", lambda b2: b1.value == b2.value)
    nested_res = tc.scoped_borrow("node", outer)
    results.append({"id": "P07", "name": "Nested Re-entrant Borrows", "passed": nested_res is True and len(tc.diagnostics) == 0, "kind": "POSITIVE"})

    # P08: Safe Early Return with Clean Release
    tc = PhantomTypeChecker()
    tc.declare_unique("res", "Resource", 100)
    tc.read_shared("res")
    results.append({"id": "P08", "name": "Read-Only Non-Mutating Access", "passed": len(tc.diagnostics) == 0, "kind": "POSITIVE"})

    # P09: Safe Re-allocation After Consumption
    tc = PhantomTypeChecker()
    tc.declare_unique("token", "Token", 1)
    tc.transfer_unique("token", "used_token")
    tc.declare_unique("token", "Token", 2)
    results.append({"id": "P09", "name": "Variable Shadowing with New Unique", "passed": len(tc.diagnostics) == 0, "kind": "POSITIVE"})

    # P10: Disjoint Channel Ownership
    tc = PhantomTypeChecker()
    tc.declare_unique("ch_pair", "ChanPair", ("tx", "rx"))
    tc.split_pair("ch_pair", "tx", "rx")
    tc.spawn_task("Sender", ["tx"])
    tc.spawn_task("Receiver", ["rx"])
    results.append({"id": "P10", "name": "Disjoint Channel Endpoints to Tasks", "passed": len(tc.diagnostics) == 0, "kind": "POSITIVE"})

    # --- 10 NEGATIVE CONFORMANCE CASES (Must Reject with Exact Diagnostic) ---
    # N01: Use After Move
    tc = PhantomTypeChecker()
    tc.declare_unique("x", "Buffer", [1, 2])
    tc.transfer_unique("x", "y")
    tc.mutate_unique("x", lambda v: v.append(3))
    n01_ok = any(d.code == "HYB-E3003" for d in tc.diagnostics)
    results.append({"id": "N01", "name": "Use After Move Rejection", "passed": n01_ok, "diag": "HYB-E3003", "kind": "NEGATIVE"})

    # N02: Concurrent Write During Shared Borrow
    tc = PhantomTypeChecker()
    tc.declare_unique("shared_val", "Data", 50)
    def illegal_mutate(s):
        return tc.mutate_unique("shared_val", lambda v: v + 1)
    tc.scoped_borrow("shared_val", illegal_mutate)
    n02_ok = any(d.code == "HYB-E3004" for d in tc.diagnostics)
    results.append({"id": "N02", "name": "Mutation During Borrow Rejection", "passed": n02_ok, "diag": "HYB-E3004", "kind": "NEGATIVE"})

    # N03: Caller Mutates Variable After Task Spawn (W-004 Falsifier Defeated)
    tc = PhantomTypeChecker()
    tc.declare_unique("captured", "SharedState", [10])
    tc.spawn_task("ChildTask", ["captured"])
    tc.mutate_unique("captured", lambda v: v.append(20)) # Caller attempts post-spawn mutation
    n03_ok = any(d.code == "HYB-E3003" for d in tc.diagnostics)
    results.append({"id": "N03", "name": "Post-Spawn Caller Mutation Rejection", "passed": n03_ok, "diag": "HYB-E3003", "kind": "NEGATIVE"})

    # N04: Double Transfer of Same Capability
    tc = PhantomTypeChecker()
    tc.declare_unique("res", "Handle", 1)
    tc.transfer_unique("res", "destA")
    tc.transfer_unique("res", "destB")
    n04_ok = any(d.code == "HYB-E3003" for d in tc.diagnostics)
    results.append({"id": "N04", "name": "Double Unique Transfer Rejection", "passed": n04_ok, "diag": "HYB-E3003", "kind": "NEGATIVE"})

    # N05: Task Spawn of Borrowed Variable
    tc = PhantomTypeChecker()
    tc.declare_unique("loaned", "Data", "info")
    def try_spawn(b):
        return tc.spawn_task("AsyncWorker", ["loaned"])
    tc.scoped_borrow("loaned", try_spawn)
    n05_ok = any(d.code == "HYB-E3001" for d in tc.diagnostics)
    results.append({"id": "N05", "name": "Spawn Borrowed Variable Rejection", "passed": n05_ok, "diag": "HYB-E3001", "kind": "NEGATIVE"})

    # N06: Mutation Through Borrowed Shared Reference
    tc = PhantomTypeChecker()
    tc.declare_unique("entity", "Entity", 99)
    def try_direct_mut(s):
        # Attacking capability by trying to invoke mutate with Shared cap
        tc.bindings["temp_ref"] = s
        return tc.mutate_unique("temp_ref", lambda v: v + 1)
    tc.scoped_borrow("entity", try_direct_mut)
    n06_ok = any(d.code == "HYB-E3006" for d in tc.diagnostics)
    results.append({"id": "N06", "name": "Mutate Through Shared Ref Rejection", "passed": n06_ok, "diag": "HYB-E3006", "kind": "NEGATIVE"})

    # N07: Transfer of Borrowed Reference
    tc = PhantomTypeChecker()
    tc.declare_unique("base", "Block", 123)
    def try_move(b):
        tc.bindings["borrow_handle"] = b
        return tc.transfer_unique("borrow_handle", "illegal_dest")
    tc.scoped_borrow("base", try_move)
    n07_ok = any(d.code == "HYB-E3002" for d in tc.diagnostics)
    results.append({"id": "N07", "name": "Move Shared Capability Rejection", "passed": n07_ok, "diag": "HYB-E3002", "kind": "NEGATIVE"})

    # N08: Split of Non-Unique Pair
    tc = PhantomTypeChecker()
    tc.declare_unique("p", "Pair", (1, 2))
    tc.transfer_unique("p", "p_moved")
    tc.split_pair("p", "a", "b")
    n08_ok = any(d.code == "HYB-E3003" for d in tc.diagnostics)
    results.append({"id": "N08", "name": "Split Expired Pair Rejection", "passed": n08_ok, "diag": "HYB-E3003", "kind": "NEGATIVE"})

    # N09: Read of Expired Capability
    tc = PhantomTypeChecker()
    tc.declare_unique("val", "Value", "data")
    tc.transfer_unique("val", "next_val")
    tc.read_shared("val")
    n09_ok = any(d.code == "HYB-E3003" for d in tc.diagnostics)
    results.append({"id": "N09", "name": "Read Expired Capability Rejection", "passed": n09_ok, "diag": "HYB-E3003", "kind": "NEGATIVE"})

    # N10: Spawn Undeclared Variable
    tc = PhantomTypeChecker()
    tc.spawn_task("GhostTask", ["non_existent_var"])
    n10_ok = any(d.code == "HYB-E1001" for d in tc.diagnostics)
    results.append({"id": "N10", "name": "Spawn Undeclared Variable Rejection", "passed": n10_ok, "diag": "HYB-E1001", "kind": "NEGATIVE"})

    return results

def benchmark_graph_vs_phantom(num_nodes: int = 1000) -> Dict[str, Any]:
    """
    Simulates:
    1. Traditional Path-Sensitive Alias-Graph Walk (freeze.rs + closures.rs): O(V * E)
       Constructs full borrow-graph, computes transitive closure, propagates loan sets across CFG edges.
    2. Zero-Alias Phantom Capability Type Checking: O(N)
       Performs single-pass type constraint solving without maintaining edge sets.
    """
    # 1. Alias Graph Walk simulation
    start_graph = time.perf_counter()
    # Build synthetic CFG with num_nodes
    alias_graph = {i: set() for i in range(num_nodes)}
    for i in range(num_nodes):
        for neighbor in range(max(0, i - 10), min(num_nodes, i + 10)):
            alias_graph[i].add(neighbor)
    # Transitive loan propagation
    loan_sets = copy.deepcopy(alias_graph)
    for _ in range(3): # 3 fixed-point iterations
        for u in alias_graph:
            for v in alias_graph[u]:
                loan_sets[u].update(loan_sets[v])
    graph_duration = time.perf_counter() - start_graph

    # 2. Phantom Capability Type Checking simulation
    start_phantom = time.perf_counter()
    tc = PhantomTypeChecker()
    for i in range(num_nodes):
        var_name = f"v_{i}"
        tc.declare_unique(var_name, "Scalar", i)
        if i % 3 == 0:
            tc.scoped_borrow(var_name, lambda s: s.value)
        elif i % 3 == 1:
            tc.mutate_unique(var_name, lambda v: v + 1)
        else:
            tc.transfer_unique(var_name, f"v_{i}_moved")
    phantom_duration = time.perf_counter() - start_phantom

    speedup = graph_duration / max(phantom_duration, 1e-9)

    return {
        "nodes": num_nodes,
        "graph_walk_seconds": graph_duration,
        "phantom_cap_seconds": phantom_duration,
        "speedup_factor": round(speedup, 2)
    }

if __name__ == "__main__":
    matrix = run_test_matrix()
    print("=" * 70)
    print("W-029 PHANTOM CAPABILITY TOKENS: CONFORMANCE TEST MATRIX")
    print("=" * 70)
    all_passed = True
    for t in matrix:
        status = "PASS" if t["passed"] else "FAIL"
        if not t["passed"]:
            all_passed = False
        diag_info = f"[{t['diag']}]" if "diag" in t else ""
        print(f"[{status}] {t['id']}: {t['name']:<40} {t['kind']:<10} {diag_info}")
    
    print("-" * 70)
    passed_count = sum(1 for t in matrix if t["passed"])
    print(f"Summary: {passed_count}/{len(matrix)} Tests Passed (Status: {'SUCCESS' if all_passed else 'FAILURE'})")
    print("=" * 70)

    print("\nRUNNING DIFFERENTIAL BENCHMARK (AST Graph Walk vs Phantom Cap)...")
    bench = benchmark_graph_vs_phantom(num_nodes=1500)
    print(f"Nodes Processed:        {bench['nodes']}")
    print(f"AST Alias-Graph Time:   {bench['graph_walk_seconds']:.5f}s")
    print(f"Phantom Cap Type Time:  {bench['phantom_cap_seconds']:.5f}s")
    print(f"Throughput Speedup:     {bench['speedup_factor']}x")
    print("=" * 70)
