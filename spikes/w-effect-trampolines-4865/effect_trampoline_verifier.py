#!/usr/bin/env python3
"""Executable static-admission, state-machine, falsifier, and ABI cost-model
verifier for Wildcard Proposal #4865:

  Algebraic Effect Trampolines Across Go<->Rust:
  Non-Reentrant Callbacks, Structured Errors, and Single-Pin Session Amortization
  (interfaces/abi-types.md §2 & §6; GUARANTEES.md Row 19).

Stdlib only. Usage:
  python3 spikes/w-effect-trampolines-4865/effect_trampoline_verifier.py --check
  python3 spikes/w-effect-trampolines-4865/effect_trampoline_verifier.py --check --json
"""

from __future__ import annotations

import argparse
from dataclasses import dataclass, field
from enum import Enum
import json
from pathlib import Path
import re
import sys

SPIKE_DIR = Path(__file__).resolve().parent
HY_DIR = SPIKE_DIR / "hy"

# Pre-registered static diagnostic codes for Proposal #4865.
CODE_UNHANDLED_EFFECT = "HYB-E3801"
CODE_REENTRANT_HANDLER = "HYB-E3802"
CODE_SCRATCH_ESCAPE = "HYB-E3803"
CODE_LINEAR_SESSION = "HYB-E3804"
CODE_NON_POD_PAYLOAD = "HYB-E3805"

# Measured crossing constants from interfaces/abi-types.md §1 & §2
# (go1.26.9 / rust 1.99.0 / linux-amd64):
ABI_FAST_CALL_NS = 2.42
ABI_PIN_PER_CALL_NS = 55.0
ABI_DEFAULT_YIELD_DISPATCH_NS = 2.50

NON_POD_TYPE_RE = re.compile(
    r"\b(?:map\s*\[|chan\b|func\b|interface\s*\{|\*|\[\s*\]\s*(?!u8\b|byte\b))"
)
EFFECT_DECL_RE = re.compile(
    r"^\s*effect\s+([A-Za-z_]\w*)\s*\(([^)]*)\)\s*->\s*\(([^)]*)\)"
)
EXTERN_SESSION_RE = re.compile(
    r"@\[effect_session\(([^)]*)\)\]\s*\n\s*extern\s+rust\s+fn\s+([A-Za-z_]\w*)\s*\("
)
HANDLE_START_RE = re.compile(
    r"\bhandle\s+([A-Za-z_]\w*)\s*\([^)]*\)\s+with\s*\{"
)
ARM_START_RE = re.compile(r"^\s*([A-Za-z_]\w*)\s*\(([^)]*)\)\s*=>\s*\{")


def _strip_line_comment(line: str) -> str:
    idx = line.find("//")
    return line if idx < 0 else line[:idx]


def _extract_brace_block(lines: list[str], start_idx: int) -> tuple[list[tuple[int, str]], int]:
    """Return [(1-based line_no, text), ...] inside the brace block starting on start_idx."""
    depth = 0
    started = False
    collected: list[tuple[int, str]] = []
    for idx in range(start_idx, len(lines)):
        code = _strip_line_comment(lines[idx])
        for ch in code:
            if ch == "{":
                depth += 1
                started = True
            elif ch == "}":
                depth -= 1
        collected.append((idx + 1, code))
        if started and depth <= 0:
            return collected, idx
    return collected, len(lines) - 1


def check_effect_boundary_admission(src: str) -> list[dict]:
    """Static Tier-0 prototype gate for @[effect_session] boundary contracts.

    Checks five pre-registered rules:
      HYB-E3801  Declared effect in @[effect_session(...)] lacks a handler arm.
      HYB-E3802  Handler arm synchronously re-enters an @[effect_session] extern fn.
      HYB-E3803  Handler arm lets a pinned scratch slice view escape handler scope.
      HYB-E3804  Linear EffectSession crosses a task boundary or is resumed after move.
      HYB-E3805  Effect ABI signature carries a non-POD / GC-pointer payload.
    """
    diags: list[dict] = []
    lines = src.splitlines()

    # 1. Effect declarations -> check POD / ABI payload safety (HYB-E3805)
    for line_no, raw in enumerate(lines, start=1):
        code = _strip_line_comment(raw)
        m = EFFECT_DECL_RE.match(code)
        if not m:
            continue
        eff_name, params, returns = m.group(1), m.group(2), m.group(3)
        for sig_part in (params, returns):
            bad = NON_POD_TYPE_RE.search(sig_part)
            if bad:
                diags.append({
                    "code": CODE_NON_POD_PAYLOAD,
                    "line": line_no,
                    "message": (
                        f"effect `{eff_name}` carries non-POD/GC-pointer payload "
                        f"`{bad.group(0).strip()}` across the effect ABI"
                    ),
                })

    # 2. Extern rust @[effect_session(...)] registry
    session_effects: dict[str, set[str]] = {}
    for m in EXTERN_SESSION_RE.finditer(src):
        effs = {e.strip() for e in m.group(1).split(",") if e.strip()}
        fn_name = m.group(2)
        session_effects[fn_name] = effs

    # 3. Inspect every `handle <extern_fn>(...) with { ... }` block
    idx = 0
    while idx < len(lines):
        code = _strip_line_comment(lines[idx])
        hm = HANDLE_START_RE.search(code)
        if not hm:
            idx += 1
            continue
        callee = hm.group(1)
        handle_line = idx + 1
        block_lines, end_idx = _extract_brace_block(lines, idx)
        handled_arms: set[str] = set()

        # Walk arms inside the handle block (skip opening line)
        b_i = 1
        while b_i < len(block_lines):
            arm_line_no, arm_code = block_lines[b_i]
            am = ARM_START_RE.match(arm_code)
            if not am:
                b_i += 1
                continue
            arm_name = am.group(1)
            arm_params = [p.strip() for p in am.group(2).split(",") if p.strip()]
            handled_arms.add(arm_name)

            # Collect lines belonging to this arm's brace body
            arm_depth = 0
            arm_started = False
            arm_body: list[tuple[int, str]] = []
            while b_i < len(block_lines):
                ln, txt = block_lines[b_i]
                for ch in txt:
                    if ch == "{":
                        arm_depth += 1
                        arm_started = True
                    elif ch == "}":
                        arm_depth -= 1
                arm_body.append((ln, txt))
                b_i += 1
                if arm_started and arm_depth <= 0:
                    break

            scratch_vars = {p for p in arm_params if "scratch" in p}
            for body_ln, body_txt in arm_body[1:]:
                # HYB-E3802: synchronous re-entry into any @[effect_session] extern fn
                for sess_fn in session_effects:
                    if re.search(rf"\b{re.escape(sess_fn)}\s*\(", body_txt):
                        diags.append({
                            "code": CODE_REENTRANT_HANDLER,
                            "line": body_ln,
                            "message": (
                                f"handler arm `{arm_name}` re-enters effect session "
                                f"`{sess_fn}` synchronously while active"
                            ),
                        })
                # HYB-E3803: pinned scratch slice assigned to outer binding, returned, or spawned
                for sv in scratch_vars:
                    escaped_assign = re.search(
                        rf"^\s*[A-Za-z_]\w*\s*=\s*{re.escape(sv)}\b", body_txt
                    )
                    escaped_return = re.search(rf"\breturn\b.*\b{re.escape(sv)}\b", body_txt)
                    escaped_task = re.search(rf"\btask\b.*\b{re.escape(sv)}\b", body_txt)
                    if escaped_assign or escaped_return or escaped_task:
                        diags.append({
                            "code": CODE_SCRATCH_ESCAPE,
                            "line": body_ln,
                            "message": (
                                f"pinned scratch view `{sv}` escapes handler arm `{arm_name}`"
                            ),
                        })

        # HYB-E3801: exhaustiveness against declared @[effect_session(...)]
        required = session_effects.get(callee, set())
        for missing in sorted(required - handled_arms):
            diags.append({
                "code": CODE_UNHANDLED_EFFECT,
                "line": handle_line,
                "message": (
                    f"effect session `{callee}` declares `{missing}` but `handle` "
                    f"provides no arm for it"
                ),
            })
        idx = end_idx + 1

    # 4. Linear EffectSession move/task-escape and post-move resume checks (HYB-E3804)
    session_vars = set(re.findall(r"\b([A-Za-z_]\w*)\s*:\s*EffectSession\b", src))
    moved_sessions: set[str] = set()
    for line_no, raw in enumerate(lines, start=1):
        code = _strip_line_comment(raw)
        for sv in sorted(session_vars):
            if re.search(rf"\btask\b.*\b(?:move\s+)?{re.escape(sv)}\b", code):
                moved_sessions.add(sv)
                diags.append({
                    "code": CODE_LINEAR_SESSION,
                    "line": line_no,
                    "message": (
                        f"linear EffectSession `{sv}` cannot escape across a `task` boundary"
                    ),
                })
            elif sv in moved_sessions and re.search(rf"\b{re.escape(sv)}\.resume\s*\(", code):
                diags.append({
                    "code": CODE_LINEAR_SESSION,
                    "line": line_no,
                    "message": (
                        f"linear EffectSession `{sv}` resumed after move/consumption"
                    ),
                })

    return diags


# ---------------------------------------------------------------------------
# Operational state-machine model of the non-reentrant ping-pong ABI
# ---------------------------------------------------------------------------

class FrameState(Enum):
    INIT = "init"
    RUNNING = "running"
    YIELDED = "yielded"
    DONE = "done"
    ERR_TERMINATED = "err_terminated"
    ABORTED = "aborted"


class ReturnTag(Enum):
    DONE = "done"
    ERR = "err"
    EFFECT_READ = "effect_read"
    EFFECT_LOG = "effect_log"


@dataclass(frozen=True)
class DecodeErr:
    """POD error payload returned by value across ReturnTag.ERR."""

    code: int
    offset: int


@dataclass
class PinLease:
    """Models a single Go runtime.Pinner lease held across an entire effect session."""

    capacity: int
    scratch: bytearray = field(init=False)
    pinned: bool = False
    pins_acquired: int = 0
    pins_released: int = 0

    def __post_init__(self) -> None:
        self.scratch = bytearray(self.capacity)

    def pin(self) -> None:
        if self.pinned:
            raise RuntimeError("HYB-RT-PIN: double pin on active session lease")
        self.pinned = True
        self.pins_acquired += 1

    def unpin(self) -> None:
        if self.pinned:
            self.pinned = False
            self.pins_released += 1


@dataclass
class RustBoundaryFrame:
    """Models a Rust-side generator/coroutine frame owned in the Rust handle table."""

    gen: int
    total_chunks: int = 4
    scratch_cap: int = 64
    emit_metrics: bool = False
    fail_on_chunk: int | None = None
    state: FrameState = FrameState.INIT
    step: int = 0
    pending_metric: bool = False
    owned_resources: int = 1  # Rust heap decoder / RAII state
    acc: int = 0
    active_rust_depth: int = 0
    max_rust_depth: int = 0

    def resume(
        self,
        gen: int,
        scratch: bytearray,
        payload_len: int,
    ) -> tuple[ReturnTag, int | DecodeErr]:
        if gen != self.gen:
            raise RuntimeError("HYB-RT-GEN: stale frame generation token rejected")
        if self.state == FrameState.RUNNING:
            raise RuntimeError("HYB-RT-REENTRANT: re-entrant resume of active frame rejected")
        if self.state in (FrameState.DONE, FrameState.ERR_TERMINATED, FrameState.ABORTED):
            raise RuntimeError("HYB-RT-CONSUMED: linear frame already terminated")
        if payload_len < 0 or payload_len > self.scratch_cap or payload_len > len(scratch):
            raise RuntimeError(
                f"HYB-RT-BOUNDS: payload_len={payload_len} exceeds scratch_cap={self.scratch_cap}"
            )

        self.state = FrameState.RUNNING
        self.active_rust_depth += 1
        self.max_rust_depth = max(self.max_rust_depth, self.active_rust_depth)
        try:
            # Consume response from previous EFFECT_READ yield
            if self.step > 0 and not self.pending_metric:
                chunk = scratch[:payload_len]
                if self.fail_on_chunk is not None and self.step == self.fail_on_chunk:
                    # Structured Result::Err teardown: RAII dropped deterministically, no unwind
                    self.owned_resources = 0
                    self.state = FrameState.ERR_TERMINATED
                    return ReturnTag.ERR, DecodeErr(code=422, offset=self.acc)
                self.acc += sum(chunk)
                if self.emit_metrics:
                    self.pending_metric = True
                    self.state = FrameState.YIELDED
                    return ReturnTag.EFFECT_LOG, self.acc

            if self.pending_metric:
                self.pending_metric = False

            if self.step < self.total_chunks:
                self.step += 1
                self.state = FrameState.YIELDED
                return ReturnTag.EFFECT_READ, self.step

            # Terminal normal completion drops Rust RAII state deterministically
            self.owned_resources = 0
            self.state = FrameState.DONE
            return ReturnTag.DONE, self.acc
        finally:
            self.active_rust_depth -= 1

    def abort(self, gen: int) -> bool:
        """Idempotent teardown bound to Go `defer frame.abort(gen)`."""
        if gen != self.gen:
            return False
        if self.state in (FrameState.DONE, FrameState.ERR_TERMINATED, FrameState.ABORTED):
            return False
        self.owned_resources = 0
        self.state = FrameState.ABORTED
        return True


def run_go_trampoline(
    frame: RustBoundaryFrame,
    read_handler,
    log_handler=None,
    *,
    enforce_defer_abort: bool = True,
) -> dict:
    """Models the emitted Go non-reentrant ping-pong trampoline with one PinLease."""
    lease = PinLease(capacity=frame.scratch_cap)
    lease.pin()
    gen = frame.gen
    payload_len = 0
    effects_handled = 0
    metrics_logged: list[int] = []
    try:
        while True:
            tag, arg = frame.resume(gen, lease.scratch, payload_len)
            if tag == ReturnTag.DONE:
                return {
                    "ok": True,
                    "value": arg,
                    "error": None,
                    "effects_handled": effects_handled,
                    "metrics_logged": metrics_logged,
                    "pins_acquired": lease.pins_acquired,
                    "pins_released_after_finally": 1,
                    "max_rust_depth": frame.max_rust_depth,
                }
            if tag == ReturnTag.ERR:
                assert isinstance(arg, DecodeErr)
                return {
                    "ok": False,
                    "value": None,
                    "error": {"code": arg.code, "offset": arg.offset},
                    "effects_handled": effects_handled,
                    "metrics_logged": metrics_logged,
                    "pins_acquired": lease.pins_acquired,
                    "pins_released_after_finally": 1,
                    "max_rust_depth": frame.max_rust_depth,
                }
            if tag == ReturnTag.EFFECT_READ:
                # Executes on Go's own splittable stack; frame is YIELDED (depth == 0)
                assert frame.active_rust_depth == 0
                effects_handled += 1
                data = read_handler(arg, frame)
                if len(data) > lease.capacity:
                    # Pass raw length to frame.resume to verify fail-closed bounds guard
                    payload_len = len(data)
                else:
                    lease.scratch[: len(data)] = data
                    payload_len = len(data)
            elif tag == ReturnTag.EFFECT_LOG:
                assert frame.active_rust_depth == 0
                effects_handled += 1
                metrics_logged.append(int(arg))
                if log_handler is not None:
                    log_handler(arg, frame)
                payload_len = 0
    finally:
        lease.unpin()
        if enforce_defer_abort:
            frame.abort(gen)


def compute_cost_model(
    counts: tuple[int, ...] = (1, 2, 8, 32, 1000),
    yield_ns: float = ABI_DEFAULT_YIELD_DISPATCH_NS,
) -> list[dict]:
    """Compare per-call pinning vs single-pin effect-session amortization."""
    rows: list[dict] = []
    for n in counts:
        per_call_ns = n * (ABI_FAST_CALL_NS + ABI_PIN_PER_CALL_NS)
        session_ns = ABI_PIN_PER_CALL_NS + n * (ABI_FAST_CALL_NS + yield_ns)
        rows.append({
            "n": n,
            "per_call_pin_ns": round(per_call_ns, 2),
            "effect_session_ns": round(session_ns, 2),
            "speedup": round(per_call_ns / session_ns, 2),
        })
    return rows


def check_all() -> dict:
    """Run P1 (static admission), P2 (state machine), P3 (cost model), and F1-F5."""
    # --- P1: Static admission on the 3 minimal .hy fixtures ---
    f01 = (HY_DIR / "01-streaming-effect-session.hy").read_text(encoding="utf-8")
    f02 = (HY_DIR / "02-structured-result-error.hy").read_text(encoding="utf-8")
    f03 = (HY_DIR / "03-rejected-effect-violations.hy").read_text(encoding="utf-8")

    d01 = check_effect_boundary_admission(f01)
    d02 = check_effect_boundary_admission(f02)
    d03 = check_effect_boundary_admission(f03)
    d03_codes = sorted({d["code"] for d in d03})
    expected_reject_codes = sorted([
        CODE_UNHANDLED_EFFECT,
        CODE_REENTRANT_HANDLER,
        CODE_SCRATCH_ESCAPE,
        CODE_LINEAR_SESSION,
        CODE_NON_POD_PAYLOAD,
    ])
    p1_ok = (len(d01) == 0) and (len(d02) == 0) and (d03_codes == expected_reject_codes)

    # --- P2: Operational state machine (streaming + interleaved effects + Result<T,E>) ---
    stream_frame = RustBoundaryFrame(gen=10, total_chunks=8, emit_metrics=True)
    stream_res = run_go_trampoline(
        stream_frame,
        lambda i, _f: bytes([i, i]),
    )
    expected_sum = sum(2 * i for i in range(1, 9))
    p2_stream_ok = (
        stream_res["ok"]
        and stream_res["value"] == expected_sum
        and stream_res["pins_acquired"] == 1
        and stream_res["max_rust_depth"] == 1
        and len(stream_res["metrics_logged"]) == 8
        and stream_frame.owned_resources == 0
        and stream_frame.state == FrameState.DONE
    )

    err_frame = RustBoundaryFrame(gen=11, total_chunks=6, fail_on_chunk=3)
    err_res = run_go_trampoline(err_frame, lambda i, _f: bytes([10, i]))
    p2_err_ok = (
        (not err_res["ok"])
        and err_res["error"] == {"code": 422, "offset": 23}
        and err_res["pins_acquired"] == 1
        and err_frame.owned_resources == 0
        and err_frame.state == FrameState.ERR_TERMINATED
    )
    p2_ok = p2_stream_ok and p2_err_ok

    # --- F1: Go handler panics mid-stream (falsifier fires without defer abort; remedy passes) ---
    def panicking_handler(i, _f):
        if i == 2:
            raise ValueError("Go callback panic mid-stream")
        return b"\x01"

    f1_leaky = RustBoundaryFrame(gen=20, total_chunks=4)
    try:
        run_go_trampoline(f1_leaky, panicking_handler, enforce_defer_abort=False)
    except ValueError:
        pass
    f1_fired = (f1_leaky.owned_resources == 1) and (f1_leaky.state == FrameState.YIELDED)

    f1_safe = RustBoundaryFrame(gen=21, total_chunks=4)
    try:
        run_go_trampoline(f1_safe, panicking_handler, enforce_defer_abort=True)
    except ValueError:
        pass
    f1_remedy = (f1_safe.owned_resources == 0) and (f1_safe.state == FrameState.ABORTED)

    # --- F2: Re-entrant resume while frame is RUNNING ---
    f2_frame = RustBoundaryFrame(gen=22, total_chunks=2)
    f2_frame.state = FrameState.RUNNING
    f2_fired = False
    try:
        f2_frame.resume(22, bytearray(8), 0)
    except RuntimeError as exc:
        f2_fired = "HYB-RT-REENTRANT" in str(exc)

    # --- F3: Stale generation token replay ---
    f3_frame = RustBoundaryFrame(gen=24, total_chunks=2)
    f3_fired = False
    try:
        f3_frame.resume(23, bytearray(8), 0)
    except RuntimeError as exc:
        f3_fired = "HYB-RT-GEN" in str(exc)

    # --- F4: Double-resume after terminal DONE / ERR / ABORTED ---
    f4_checks = []
    for terminal_frame in (stream_frame, err_frame, f1_safe):
        caught = False
        try:
            terminal_frame.resume(terminal_frame.gen, bytearray(8), 0)
        except RuntimeError as exc:
            caught = "HYB-RT-CONSUMED" in str(exc)
        f4_checks.append(caught)
    f4_fired = all(f4_checks)

    # --- F5: Scratch-buffer length overflow ---
    f5_frame = RustBoundaryFrame(gen=25, total_chunks=2, scratch_cap=16)
    f5_fired = False
    try:
        run_go_trampoline(f5_frame, lambda _i, _f: b"x" * 32, enforce_defer_abort=True)
    except RuntimeError as exc:
        f5_fired = "HYB-RT-BOUNDS" in str(exc) and f5_frame.owned_resources == 0

    falsifiers_ok = all([f1_fired, f1_remedy, f2_fired, f3_fired, f4_fired, f5_fired])

    # --- P3: Cost model & sensitivity sweep ---
    cost_baseline = compute_cost_model(yield_ns=ABI_DEFAULT_YIELD_DISPATCH_NS)
    cost_conservative = compute_cost_model(yield_ns=10.0)
    cost_stress = compute_cost_model(yield_ns=20.0)
    p3_ok = (
        cost_baseline[1]["n"] == 2
        and cost_baseline[1]["speedup"] > 1.5
        and cost_baseline[-1]["speedup"] > 10.0
        and cost_stress[1]["speedup"] > 1.0
    )

    verified = p1_ok and p2_ok and p3_ok and falsifiers_ok
    return {
        "verified": verified,
        "p1_static_admission": {
            "pass": p1_ok,
            "fixture_01_diags": len(d01),
            "fixture_02_diags": len(d02),
            "fixture_03_codes": d03_codes,
        },
        "p2_state_machine": {
            "pass": p2_ok,
            "stream_sum": stream_res["value"],
            "stream_pins": stream_res["pins_acquired"],
            "stream_max_rust_depth": stream_res["max_rust_depth"],
            "stream_metrics_logged": len(stream_res["metrics_logged"]),
            "err_payload": err_res["error"],
            "err_leaked_resources": err_frame.owned_resources,
        },
        "p3_cost_model": {
            "pass": p3_ok,
            "baseline_2_5ns": cost_baseline,
            "conservative_10ns": cost_conservative,
            "stress_20ns": cost_stress,
        },
        "falsifiers": {
            "pass": falsifiers_ok,
            "F1_panic_leak_fired": f1_fired,
            "F1_defer_abort_remedy": f1_remedy,
            "F2_reentrant_rejected": f2_fired,
            "F3_stale_gen_rejected": f3_fired,
            "F4_double_resume_rejected": f4_fired,
            "F5_scratch_overflow_rejected": f5_fired,
        },
    }


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true", help="run all pre-registered gates")
    parser.add_argument("--json", action="store_true", help="emit JSON report")
    args = parser.parse_args(argv)

    res = check_all()
    if args.json:
        print(json.dumps(res, indent=2, sort_keys=True))
    else:
        print(
            f"W-4865 Effect Trampolines Spike: verified={res['verified']} "
            f"(P1={res['p1_static_admission']['pass']} "
            f"P2={res['p2_state_machine']['pass']} "
            f"P3={res['p3_cost_model']['pass']} "
            f"F1-F5={res['falsifiers']['pass']})"
        )
        print(
            f"  P1: 01={res['p1_static_admission']['fixture_01_diags']} diags, "
            f"02={res['p1_static_admission']['fixture_02_diags']} diags, "
            f"03={res['p1_static_admission']['fixture_03_codes']}"
        )
        print(
            f"  P2: stream_sum={res['p2_state_machine']['stream_sum']} "
            f"pins={res['p2_state_machine']['stream_pins']} "
            f"max_rust_depth={res['p2_state_machine']['stream_max_rust_depth']} "
            f"err={res['p2_state_machine']['err_payload']} "
            f"leaks={res['p2_state_machine']['err_leaked_resources']}"
        )
        print("  P3 baseline (fast=2.42ns, pin=55.0ns, yield=2.50ns):")
        for row in res["p3_cost_model"]["baseline_2_5ns"]:
            print(
                f"    N={row['n']:4d}: per_call={row['per_call_pin_ns']:8.2f}ns | "
                f"session={row['effect_session_ns']:7.2f}ns | speedup={row['speedup']:5.2f}x"
            )
        print(f"  F1-F5: {res['falsifiers']}")
    return 0 if res["verified"] else 1


if __name__ == "__main__":
    sys.exit(main())
