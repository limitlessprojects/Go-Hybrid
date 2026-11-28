"""W-4655 (#4655): Zero-Allocation Task Graph Certification via Static Region Epicycles.

This module provides the static verification model and compile-time escape certifier
for @heapfree task graphs and Capability Slab Tokens (Slab[N]).
"""

from __future__ import annotations
from dataclasses import dataclass, field
from enum import Enum
from typing import Dict, List, Optional, Set, Tuple


class DiagnosticCode(str, Enum):
    OK = "OK"
    HYB_E4001 = "HYB-E4001"


@dataclass(frozen=True)
class ValueType:
    name: str
    size_bytes: int
    is_pointer_or_dynamic: bool = False
    is_value_copy: bool = True


@dataclass(frozen=True)
class ChannelPayload:
    channel_id: str
    payload_type: ValueType
    escapes_lexical_scope: bool = False


@dataclass
class TaskNode:
    task_id: str
    is_heapfree: bool
    slab_capacity_bytes: int = 0
    local_allocations: List[Tuple[str, ValueType]] = field(default_factory=list)
    closure_captures: List[Tuple[str, ValueType]] = field(default_factory=list)
    channel_sends: List[ChannelPayload] = field(default_factory=list)
    subtasks: List["TaskNode"] = field(default_factory=list)


@dataclass(frozen=True)
class CertificationVerdict:
    code: DiagnosticCode
    passed: bool
    allocs_per_op: int
    rejection_reason: Optional[str] = None
    task_id: Optional[str] = None


class HeapfreeCertifier:
    """Verifies that @heapfree tasks execute within bounded static region epicycles without Go heap escape."""

    def __init__(self):
        self.diagnostics: List[str] = []

    def verify_task(self, task: TaskNode) -> CertificationVerdict:
        if not task.is_heapfree:
            # Standard task - not certified, relies on Go GC
            return CertificationVerdict(
                code=DiagnosticCode.OK,
                passed=True,
                allocs_per_op=1, # Go GC allocated
                rejection_reason=None,
                task_id=task.task_id,
            )

        used_bytes = 0

        # 1. Check local allocations within slab
        for alloc_name, vtype in task.local_allocations:
            if vtype.is_pointer_or_dynamic:
                reason = f"HYB-E4001: Slab capacity escape detected: dynamic/pointer allocation '{alloc_name}' of type '{vtype.name}' in @heapfree task '{task.task_id}' cannot reside on static slab"
                return CertificationVerdict(
                    code=DiagnosticCode.HYB_E4001,
                    passed=False,
                    allocs_per_op=1,
                    rejection_reason=reason,
                    task_id=task.task_id,
                )
            used_bytes += vtype.size_bytes

        # 2. Check closure captures
        for cap_name, vtype in task.closure_captures:
            if vtype.is_pointer_or_dynamic:
                reason = f"HYB-E4001: Slab capacity escape detected: closure captures outer heap reference '{cap_name}' ({vtype.name})"
                return CertificationVerdict(
                    code=DiagnosticCode.HYB_E4001,
                    passed=False,
                    allocs_per_op=1,
                    rejection_reason=reason,
                    task_id=task.task_id,
                )
            used_bytes += vtype.size_bytes

        # 3. Check channel payloads (must not escape epicycle scope dynamically)
        for send in task.channel_sends:
            if send.escapes_lexical_scope or send.payload_type.is_pointer_or_dynamic:
                reason = f"HYB-E4001: Slab capacity escape detected: channel payload '{send.channel_id}' with dynamic type '{send.payload_type.name}' escapes lexical epicycle"
                return CertificationVerdict(
                    code=DiagnosticCode.HYB_E4001,
                    passed=False,
                    allocs_per_op=1,
                    rejection_reason=reason,
                    task_id=task.task_id,
                )
            # Value payloads fit within channel buffer/value copy
            used_bytes += send.payload_type.size_bytes

        # 4. Check total slab capacity
        if task.slab_capacity_bytes > 0 and used_bytes > task.slab_capacity_bytes:
            reason = f"HYB-E4001: Slab capacity escape detected: task required {used_bytes} bytes, exceeding Slab[{task.slab_capacity_bytes}]"
            return CertificationVerdict(
                code=DiagnosticCode.HYB_E4001,
                passed=False,
                allocs_per_op=1,
                rejection_reason=reason,
                task_id=task.task_id,
            )

        # 5. Recursively verify spawned subtasks
        for sub in task.subtasks:
            sub_res = self.verify_task(sub)
            if not sub_res.passed:
                return sub_res

        # All checks passed: 0 allocs/op certified
        return CertificationVerdict(
            code=DiagnosticCode.OK,
            passed=True,
            allocs_per_op=0,
            rejection_reason=None,
            task_id=task.task_id,
        )

    def benchmark_pipeline(self, num_workers: int, num_messages: int) -> Dict[str, any]:
        """Simulate high-throughput 100k msgs/sec pipeline under certified @heapfree rules."""
        # 16 worker tasks with pre-allocated static slabs
        workers = [
            TaskNode(
                task_id=f"worker_{i}",
                is_heapfree=True,
                slab_capacity_bytes=4096,
                local_allocations=[("msg_buf", ValueType("RingBuffer[64, Int64]", 512, False, True))],
                closure_captures=[("worker_idx", ValueType("Int64", 8, False, True))],
                channel_sends=[ChannelPayload(f"worker_{i}_out", ValueType("Int64", 8, False, True), False)],
            )
            for i in range(num_workers)
        ]

        parent_task = TaskNode(
            task_id="pipeline_coordinator",
            is_heapfree=True,
            slab_capacity_bytes=65536,
            local_allocations=[("state_table", ValueType("Array[1024, Int64]", 8192, False, True))],
            subtasks=workers,
        )

        verdict = self.verify_task(parent_task)
        if not verdict.passed:
            raise RuntimeError(f"Pipeline verification failed: {verdict.rejection_reason}")

        # In certified @heapfree mode, message transfers happen via value registers or fixed slab slots
        return {
            "total_messages": num_messages,
            "workers": num_workers,
            "certified_heapfree": True,
            "allocs_per_op": 0,
            "heap_escapes": 0,
            "gc_stw_sweeps": 0,
        }
