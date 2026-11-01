"""W-2015 (#2015): O18 GoBench Spawn Census & O4.0 Closure/Method-Value Prover."""

from __future__ import annotations
from dataclasses import dataclass
from typing import Dict, List, Tuple


@dataclass(frozen=True)
class SpawnSite:
    kernel_id: str
    kind: str  # "plain_fn" | "method_value" | "closure"
    receiver_sendable: bool = False
    captures_sendable: bool = False
    mutates_outer_alias: bool = False


@dataclass(frozen=True)
class SpawnAuditVerdict:
    kernel_id: str
    baseline_o40_provable: bool
    baseline_code: str
    extended_o40_provable: bool
    extended_code: str
    requires_managed_hatch: bool


def evaluate_spawn_site(site: SpawnSite) -> SpawnAuditVerdict:
    """Compare baseline O4.0 (top-level fn only) vs O4.0-ext (method-value + closure lift)."""
    if site.kind == "plain_fn":
        if site.mutates_outer_alias:
            return SpawnAuditVerdict(site.kernel_id, False, "HYB-E3001_SHARED_MUT", False, "HYB-E3001_SHARED_MUT", True)
        return SpawnAuditVerdict(site.kernel_id, True, "OK_TOPLEVEL_FN", True, "OK_TOPLEVEL_FN", False)

    # Baseline O4.0 rejects all method_value and closure spawns with HYB-E3005
    baseline_code = "HYB-E3005_UNPROVABLE_CALLEE"
    if site.mutates_outer_alias:
        return SpawnAuditVerdict(site.kernel_id, False, baseline_code, False, "HYB-E3001_CAPTURED_RACE", True)

    if site.kind == "method_value":
        if site.receiver_sendable:
            return SpawnAuditVerdict(site.kernel_id, False, baseline_code, True, "OK_LIFTED_METHOD_VALUE", False)
        return SpawnAuditVerdict(site.kernel_id, False, baseline_code, False, "HYB-E3005_UNSENDABLE_RECEIVER", True)

    if site.kind == "closure":
        if site.captures_sendable:
            return SpawnAuditVerdict(site.kernel_id, False, baseline_code, True, "OK_LIFTED_IIFE_CLOSURE", False)
        return SpawnAuditVerdict(site.kernel_id, False, baseline_code, False, "HYB-E3005_UNSENDABLE_CAPTURE", True)

    return SpawnAuditVerdict(site.kernel_id, False, baseline_code, False, baseline_code, True)


def summarize_gobench_census(kernels: Dict[str, List[SpawnSite]]) -> Dict[str, object]:
    """Compute the O18 tripwire metrics over the GoBench GoKer kernel map."""
    total_kernels = len(kernels)
    spawn_kernels = 0
    zero_provable_baseline_kernels = 0
    counts = {"plain_fn": 0, "method_value": 0, "closure": 0}
    total_spawns = 0
    baseline_unprovable = 0
    extended_provable = 0

    for kid, sites in kernels.items():
        if not sites:
            continue
        spawn_kernels += 1
        kernel_has_baseline_provable = False
        for s in sites:
            counts[s.kind] = counts.get(s.kind, 0) + 1
            total_spawns += 1
            verdict = evaluate_spawn_site(s)
            if verdict.baseline_o40_provable:
                kernel_has_baseline_provable = True
            else:
                baseline_unprovable += 1
            if verdict.extended_o40_provable:
                extended_provable += 1
        if not kernel_has_baseline_provable:
            zero_provable_baseline_kernels += 1

    unprovable_ratio = round(baseline_unprovable / total_spawns, 3) if total_spawns else 0.0
    return {
        "total_kernels": total_kernels,
        "spawn_bearing_kernels": spawn_kernels,
        "total_spawns": total_spawns,
        "plain_fn_spawns": counts["plain_fn"],
        "method_spawns": counts["method_value"],
        "closure_spawns": counts["closure"],
        "baseline_unprovable_spawns": baseline_unprovable,
        "baseline_unprovable_ratio": unprovable_ratio,
        "zero_provable_baseline_kernels": zero_provable_baseline_kernels,
        "o18_tripwire_fired": unprovable_ratio > 0.25,
        "extended_o40_provable_spawns": extended_provable,
    }


def build_reference_gobench_35() -> Dict[str, List[SpawnSite]]:
    """Construct the exact 35-kernel GoBench GoKer shape distribution (4 plain, 17 method, 52 closure = 73)."""
    kernels: Dict[str, List[SpawnSite]] = {"goker_no_spawn_01": []}
    # 4 kernels with 1 plain_fn spawn each (4 plain_fn total)
    for i in range(1, 5):
        kernels[f"goker_plain_{i:02d}"] = [SpawnSite(f"goker_plain_{i:02d}", "plain_fn")]
    # 17 method_value spawns across 10 kernels (12 sendable receivers, 5 racing/unsendable)
    method_Touch = [2, 2, 2, 2, 2, 2, 2, 1, 1, 1]
    idx = 0
    for k_i, cnt in enumerate(method_Touch, 1):
        sites = []
        for _ in range(cnt):
            idx += 1
            sites.append(SpawnSite(f"goker_method_{k_i:02d}", "method_value", receiver_sendable=(idx <= 12)))
        kernels[f"goker_method_{k_i:02d}"] = sites
    # 52 closure spawns across 20 kernels (including etcd/4876 with 2 closure spawns)
    kernels["etcd/4876"] = [
        SpawnSite("etcd/4876", "closure", captures_sendable=True, mutates_outer_alias=False),
        SpawnSite("etcd/4876", "closure", captures_sendable=False, mutates_outer_alias=True),
    ]
    remaining_closures = 50
    for k_i in range(1, 20):
        cnt = 3 if k_i <= 12 else 2
        if k_i == 19:
            cnt = remaining_closures
        remaining_closures -= cnt
        kernels[f"goker_closure_{k_i:02d}"] = [
            SpawnSite(f"goker_closure_{k_i:02d}", "closure", captures_sendable=(j == 0))
            for j in range(cnt)
        ]
    return kernels
