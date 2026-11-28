#!/usr/bin/env python3
"""seal_verifier.py — Executable Tier-0 reference model & live-Go verifier for
Monotonic Ownership-to-Value Sealing (`seal move x -> sealed[T]`, O5.7 / Class 4b).

Models the exact interaction with `compiler/crates/hy-check/src/`:
  - `freeze.rs` (`storage_component` over `aliases U may_share`, `MoveKind`)
  - `sendability.rs` (`O4` classes + Class 4b `Sealed(inner)` + `HYB-E3015` `is_sealable`)
  - `closures.rs` (Row 5 & Row 9 foreign callback check-site `HYB-E3013`)
  - `hy-emit` (`R1`: mandatory 3-index capacity clip `x[:len(x):len(x)]` on slice seal)
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass, field
import json
import pathlib
import subprocess
import sys
import tempfile
from typing import Dict, List, Optional, Set, Tuple

SPIKE_DIR = pathlib.Path(__file__).resolve().parent
WORK_DIR = SPIKE_DIR / "work"
HY_DIR = SPIKE_DIR / "hy"


@dataclass(frozen=True)
class Shape:
    kind: str  # Prim, String, Slice, Array, Map, Struct, Option, Sealed, Closure, Chan, Interface, ForeignOpaque, Pointer
    args: Tuple["Shape", ...] = ()
    fields: Tuple[Tuple[str, "Shape"], ...] = ()

    def is_sendable_by_type(self) -> bool:
        if self.kind in {"Prim", "String", "Chan", "Sealed"}:
            return True
        if self.kind in {"Array", "Option"}:
            return all(a.is_sendable_by_type() for a in self.args)
        if self.kind == "Struct":
            return all(f_shape.is_sendable_by_type() for _, f_shape in self.fields)
        return False

    def is_sealable(self) -> bool:
        """Remedy R3 (`HYB-E3015`): recursive representation-purity predicate.
        Admits pure value/container trees whose mutation surface is erased by
        `sealed[T]`; rejects interior closures, channels, interfaces, opaque
        foreign handles, and raw mutable pointers.
        """
        if self.kind in {"Prim", "String", "Sealed"}:
            return True
        if self.kind in {"Slice", "Array", "Option"}:
            return all(a.is_sealable() for a in self.args)
        if self.kind == "Map":
            return len(self.args) == 2 and self.args[0].is_sealable() and self.args[1].is_sealable()
        if self.kind == "Struct":
            return all(f_shape.is_sealable() for _, f_shape in self.fields)
        return False


@dataclass
class Binding:
    name: str
    shape: Shape
    non_owned_origin: bool = False
    managed_taint: bool = False
    is_const: bool = False
    state: str = "live"  # "live" | "moved" | "sealed_source"


@dataclass
class FnModel:
    bindings: Dict[str, Binding] = field(default_factory=dict)
    aliases: Dict[str, str] = field(default_factory=dict)
    may_share: Dict[str, Set[str]] = field(default_factory=dict)
    diagnostics: List[str] = field(default_factory=list)
    emitted_clips: List[str] = field(default_factory=list)

    def declare(
        self,
        name: str,
        shape: Shape,
        *,
        non_owned: bool = False,
        managed: bool = False,
        is_const: bool = False,
    ) -> None:
        self.bindings[name] = Binding(
            name=name,
            shape=shape,
            non_owned_origin=non_owned,
            managed_taint=managed,
            is_const=is_const,
        )

    def link_may_share(self, a: str, b: str) -> None:
        self.may_share.setdefault(a, set()).add(b)
        self.may_share.setdefault(b, set()).add(a)

    def storage_component(self, seed: str) -> Set[str]:
        """Mirrors `freeze.rs:43` `storage_component(st, seed)` across `aliases` and `may_share`."""
        seen: Set[str] = set()
        stack = [seed]
        while stack:
            cur = stack.pop()
            if cur in seen:
                continue
            seen.add(cur)
            parent = self.aliases.get(cur)
            if parent and parent not in seen:
                stack.append(parent)
            for child, par in self.aliases.items():
                if par == cur and child not in seen:
                    stack.append(child)
            for peer in self.may_share.get(cur, set()):
                if peer not in seen:
                    stack.append(peer)
        return seen

    def use_read(self, name: str) -> None:
        b = self.bindings[name]
        if b.state in {"moved", "sealed_source"}:
            # Both `move` (corpus #19) and `seal move` consume the mutable source binding;
            # post-seal reads go through the new `sealed[T]` binding, never the consumed source.
            self.diagnostics.append("HYB-E3003")

    def use_write(self, name: str) -> None:
        b = self.bindings[name]
        if b.is_const or b.shape.kind == "Sealed":
            self.diagnostics.append("HYB-E3006")
            return
        if b.state in {"moved", "sealed_source"}:
            self.diagnostics.append("HYB-E3003")

    def move_send(self, name: str) -> None:
        b = self.bindings[name]
        if b.non_owned_origin or b.is_const:
            self.diagnostics.append("HYB-E3004")
            return
        if b.state != "live":
            self.diagnostics.append("HYB-E3003")
            return
        for peer in self.storage_component(name):
            self.bindings[peer].state = "moved"

    def plain_send_or_spawn(self, name: str) -> None:
        b = self.bindings[name]
        if b.state != "live":
            self.diagnostics.append("HYB-E3003")
            return
        if not (b.is_const or b.managed_taint or b.shape.is_sendable_by_type()):
            self.diagnostics.append("HYB-E3001")

    def seal_move(self, src: str, dst: str) -> None:
        """O5.7: `let dst = seal move src` -> consumes `storage_component(src)`
        and binds `dst` as `Shape('Sealed', (src.shape,))` (Sendable Class 4b).
        """
        b = self.bindings[src]
        if b.state != "live":
            self.diagnostics.append("HYB-E3003")
            return
        if b.non_owned_origin or b.is_const:
            self.diagnostics.append("HYB-E3004")
            return
        if b.managed_taint or not b.shape.is_sealable():
            self.diagnostics.append("HYB-E3015")
            return
        # Remedy R2: freeze entire `aliases U may_share` component against future use/writes.
        for peer in self.storage_component(src):
            self.bindings[peer].state = "sealed_source"
        # Remedy R1: slice sealing emits mandatory 3-index capacity clip.
        if b.shape.kind == "Slice":
            self.emitted_clips.append(f"{src}[:len({src}):len({src})]")
        self.declare(dst, Shape("Sealed", (b.shape,)))

    def foreign_callback_capture(self, captured: str, *, writes: bool, in_managed_interop: bool = False) -> None:
        """Strengthened GUARANTEES Row 5 & Row 9 check-site:
        Any mutable non-const capture (even read-only!) escaping into a foreign
        Go callback (`http.HandleFunc`) rejects with HYB-E3013 unless `sealed[T]`
        or explicitly inside `managed("...", "interop")`.
        """
        if in_managed_interop:
            return
        b = self.bindings[captured]
        if b.state != "live":
            self.diagnostics.append("HYB-E3003")
            return
        if writes:
            self.diagnostics.append("HYB-E3013")
            return
        if not (b.is_const or b.shape.is_sendable_by_type()):
            self.diagnostics.append("HYB-E3013")


MAP_STR_INT = Shape("Map", (Shape("Prim"), Shape("Prim")))
SLICE_INT = Shape("Slice", (Shape("Prim"),))


def check_corpus19_move_then_sender_read() -> List[str]:
    m = FnModel()
    m.declare("agg", MAP_STR_INT)
    m.move_send("agg")
    m.use_read("agg")  # corpus #19 line 72: `n := len(agg)` -> REJECT HYB-E3003
    return m.diagnostics


def check_d1_seal_broadcast_and_sender_read() -> Dict[str, object]:
    m = FnModel()
    m.declare("agg", MAP_STR_INT)
    m.declare("tags", SLICE_INT)
    m.seal_move("agg", "snap")
    m.seal_move("tags", "sealed_tags")
    m.plain_send_or_spawn("snap")
    m.plain_send_or_spawn("snap")  # 2nd task broadcast — legal without `move`!
    m.use_read("snap")  # sender continues reading `snap.len()` — 0 diagnostics!
    return {
        "diagnostics": m.diagnostics,
        "emitted_slice_clip": m.emitted_clips[0] if m.emitted_clips else "",
    }


def check_post_seal_direct_write() -> List[str]:
    m = FnModel()
    m.declare("agg", MAP_STR_INT)
    m.seal_move("agg", "snap")
    m.use_write("agg")
    return m.diagnostics


def check_seal_non_owned_param() -> List[str]:
    m = FnModel()
    m.declare("param_map", MAP_STR_INT, non_owned=True)
    m.seal_move("param_map", "snap")
    return m.diagnostics


def verify_r1_three_index_clip_invariant() -> bool:
    m = FnModel()
    m.declare("buf", SLICE_INT)
    m.seal_move("buf", "snap")
    return m.diagnostics == [] and m.emitted_clips == ["buf[:len(buf):len(buf)]"]


def check_f2_pre_seal_may_share_write() -> List[str]:
    m = FnModel()
    m.declare("buf", SLICE_INT)
    m.declare("sub", SLICE_INT)
    m.link_may_share("buf", "sub")  # `sub := buf[:1]` before `seal move buf`
    m.seal_move("buf", "snap")
    m.use_write("sub")  # `sub[0] = 99` -> HYB-E3003 via Remedy R2!
    return m.diagnostics


def check_f3_unsealable_shapes() -> List[str]:
    diags: List[str] = []
    # 1. Struct containing stateful closure
    m1 = FnModel()
    m1.declare("cfg", Shape("Struct", fields=(("name", Shape("String")), ("next_id", Shape("Closure")))))
    m1.seal_move("cfg", "s1")
    diags.extend(m1.diagnostics)
    # 2. Struct containing channel
    m2 = FnModel()
    m2.declare("pkt", Shape("Struct", fields=(("ch", Shape("Chan")),)))
    m2.seal_move("pkt", "s2")
    diags.extend(m2.diagnostics)
    # 3. Interface value (`io.Reader`)
    m3 = FnModel()
    m3.declare("rdr", Shape("Interface"))
    m3.seal_move("rdr", "s3")
    diags.extend(m3.diagnostics)
    # 4. Managed-tainted map
    m4 = FnModel()
    m4.declare("tainted", MAP_STR_INT, managed=True)
    m4.seal_move("tainted", "s4")
    diags.extend(m4.diagnostics)
    return diags


def check_row9_unsealed_readonly_capture() -> List[str]:
    m = FnModel()
    m.declare("routes", MAP_STR_INT)
    m.foreign_callback_capture("routes", writes=False)
    return m.diagnostics


def check_row9_sealed_capture() -> List[str]:
    m = FnModel()
    m.declare("routes", MAP_STR_INT)
    m.seal_move("routes", "sealed_routes")
    m.foreign_callback_capture("sealed_routes", writes=False)
    return m.diagnostics


def check_hy_fixtures() -> bool:
    expected = {
        "01-d1-corpus19-move-vs-seal_accept.hy": "EXPECTED-VERDICT: ACCEPT",
        "02-row9-readonly-capture_reject.hy": "EXPECTED-VERDICT: REJECT HYB-E3013",
        "03-falsifiers-e3003-e3015_reject.hy": "EXPECTED-VERDICT: REJECT HYB-E3003 HYB-E3015",
    }
    for fname, marker in expected.items():
        fpath = HY_DIR / fname
        if not fpath.is_file():
            return False
        txt = fpath.read_text(encoding="utf-8")
        if marker not in txt:
            return False
    return True


def run_live_go_checks() -> Tuple[bool, str]:
    # 1. `go test -v -count=1 ./...` (includes negative compile-rejection tests)
    res = subprocess.run(
        ["go", "test", "-v", "-count=1", "./..."],
        cwd=str(WORK_DIR),
        capture_output=True,
        text=True,
        timeout=180,
    )
    if res.returncode != 0:
        return False, f"go test failed:\n{res.stdout}\n{res.stderr}"

    # 2. `go vet ./...`
    vet = subprocess.run(
        ["go", "vet", "./..."],
        cwd=str(WORK_DIR),
        capture_output=True,
        text=True,
        timeout=60,
    )
    if vet.returncode != 0:
        return False, f"go vet failed:\n{vet.stdout}\n{vet.stderr}"

    # 3. Direct negative witness verification via `go build`
    with tempfile.TemporaryDirectory() as td:
        out_bin = str(pathlib.Path(td) / "neg.bin")
        for neg in (
            "negative/uam_write_after_seal_map.go",
            "negative/uam_write_after_seal_slice.go",
            "negative/mutate_sealed_map_projection.go",
        ):
            nb = subprocess.run(
                ["go", "build", "-o", out_bin, neg],
                cwd=str(WORK_DIR),
                capture_output=True,
                text=True,
                timeout=60,
            )
            if nb.returncode == 0:
                return False, f"negative file compiled unexpectedly: {neg}"

    return True, "ok"


def check_all(live_go: bool = False) -> Dict[str, object]:
    checks = {
        "corpus19_stays_reject": check_corpus19_move_then_sender_read() == ["HYB-E3003"],
        "d1_seal_broadcast_accepts": check_d1_seal_broadcast_and_sender_read()["diagnostics"] == [],
        "post_seal_write_rejects": check_post_seal_direct_write() == ["HYB-E3003"],
        "non_owned_param_rejects": check_seal_non_owned_param() == ["HYB-E3004"],
        "r1_three_index_clip": verify_r1_three_index_clip_invariant(),
        "r2_may_share_freeze": check_f2_pre_seal_may_share_write() == ["HYB-E3003"],
        "r3_is_sealable_purity": check_f3_unsealable_shapes() == ["HYB-E3015"] * 4,
        "row9_flip": check_row9_unsealed_readonly_capture() == ["HYB-E3013"] and check_row9_sealed_capture() == [],
        "hy_fixtures_present": check_hy_fixtures(),
    }
    go_detail = "skipped"
    if live_go:
        ok_go, go_detail = run_live_go_checks()
        checks["live_go_suite"] = ok_go
    verified = all(checks.values())
    return {"verified": verified, "checks": checks, "go_detail": go_detail}


def main() -> None:
    ap = argparse.ArgumentParser(description=__doc__)
    ap.add_argument("--check", action="store_true", help="run static Tier-0 model checks")
    ap.add_argument("--live-go", action="store_true", help="also run Go test, vet, and negative build gates")
    args = ap.parse_args()
    res = check_all(live_go=args.live_go)
    print(json.dumps(res, indent=2))
    sys.exit(0 if res["verified"] else 1)


if __name__ == "__main__":
    main()
