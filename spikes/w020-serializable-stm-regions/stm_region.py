#!/usr/bin/env python3
"""W-020 (#417) Spike: Serializable Shared-State Regions via Admitted Transactions."""

from dataclasses import dataclass, field
from typing import Dict, List, Optional, Tuple

FORBIDDEN_IO = {"fmt.println", "fmt.printf", "os.open", "net.dial"}
FORBIDDEN_FOREIGN = {"cgo.call", "unknown_callee"}


@dataclass(frozen=True)
class CellValue:
    val: int
    version: int = 0


@dataclass(frozen=True)
class RegionRoot:
    cells: Dict[str, CellValue]
    epoch: int = 0


@dataclass
class TxAttempt:
    snapshot: RegionRoot
    read_versions: Dict[str, int] = field(default_factory=dict)
    write_set: Dict[str, int] = field(default_factory=dict)
    aborted: bool = False
    mode: str = "full"  # "full", "write_only", "stale_base", "no_ryw"

    def read(self, cell: str) -> int:
        if self.mode != "no_ryw" and cell in self.write_set:
            if cell not in self.read_versions:
                self.read_versions[cell] = self.snapshot.cells[cell].version
            return self.write_set[cell]
        cv = self.snapshot.cells[cell]
        self.read_versions.setdefault(cell, cv.version)
        return cv.val

    def write(self, cell: str, val: int) -> None:
        self.write_set[cell] = val

    def abort(self) -> None:
        self.aborted = True


class TransactionalRegion:
    """Immutable-root transactional state region with configurable validation controls."""

    def __init__(self, initial: Dict[str, int], mode: str = "full") -> None:
        self.root = RegionRoot(
            cells={k: CellValue(v, 0) for k, v in initial.items()},
            epoch=0,
        )
        self.mode = mode

    def begin(self) -> TxAttempt:
        return TxAttempt(snapshot=self.root, mode=self.mode)

    def commit(self, tx: TxAttempt) -> Tuple[bool, str]:
        if tx.aborted:
            return False, "ABORTED_EXPLICIT"
        cur = self.root
        if self.mode == "write_only":
            for cell in tx.write_set:
                snap_v = tx.read_versions.get(cell, tx.snapshot.cells[cell].version)
                if cur.cells[cell].version != snap_v:
                    return False, "CONFLICT_WRITE_VERSION"
        else:
            for cell, snap_v in tx.read_versions.items():
                if cur.cells[cell].version != snap_v:
                    return False, "CONFLICT_READ_SET"
            for cell in tx.write_set:
                if cell not in tx.read_versions:
                    if cur.cells[cell].version != tx.snapshot.cells[cell].version:
                        return False, "CONFLICT_BLIND_WRITE"

        if not tx.write_set:
            return True, "COMMITTED_READ_ONLY"

        base_cells = dict(tx.snapshot.cells) if self.mode == "stale_base" else dict(cur.cells)
        for cell, new_val in tx.write_set.items():
            old_ver = cur.cells[cell].version
            base_cells[cell] = CellValue(val=new_val, version=old_ver + 1)
        self.root = RegionRoot(cells=base_cells, epoch=cur.epoch + 1)
        return True, "COMMITTED"

    def snapshot_values(self) -> Dict[str, int]:
        return {k: v.val for k, v in self.root.cells.items()}


@dataclass
class AdmissionResult:
    admitted: bool
    diagnostics: List[str] = field(default_factory=list)


def check_tx_admission(stmts: List[Dict[str, object]]) -> AdmissionResult:
    """Enforces compile-time effect admission inside a transactional region block."""
    diags: List[str] = []
    for st in stmts:
        kind = str(st.get("op", ""))
        target = str(st.get("target", ""))
        if kind == "io" or target in FORBIDDEN_IO:
            diags.append(f"HYB-E6001: I/O operation '{target or kind}' forbidden in transactional region")
        elif kind in ("chan_send", "chan_recv", "select"):
            diags.append(f"HYB-E6002: channel operation '{kind}' forbidden in transactional region")
        elif kind == "task_spawn":
            diags.append("HYB-E6003: task spawn forbidden inside transactional region")
        elif kind in ("nested_tx", "mutable_capture", "interior_ptr", "map_slice_cell") or target in FORBIDDEN_FOREIGN:
            diags.append(f"HYB-E6004: unadmitted effect or non-scalar escape '{target or kind}' (fail-closed)")
    return AdmissionResult(admitted=(len(diags) == 0), diagnostics=diags)
