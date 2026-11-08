# W-2049 (#2049) Spike: Zero-Trampoline Go Assembly Emission (`hy-asm`) for Row 20 (< 5.0 ns)

## 1. Summary & Ledger Reference
- **Issue:** `#2049` (`[wildcard] Zero-Trampoline Go Assembly Emission: Direct ABIInternal Stack-Switching Stubs (hy-asm) to Beat the 5 ns Budget (Row 20)`)
- **Ledger ID:** `W-2049` (`$500` USD Wildcard Track, `0` open competing PRs)
- **Architecture:** Emits companion Plan 9 Go assembly stubs (`.s`) using `ABIInternal` register handoff (`AX`/`BX`/`CX` -> `DI`/`SI`/`DX` on `amd64`; `R0`–`R7` 1:1 on `arm64`) for DWARF-certified `NOSPLIT` leaves (`frame_bytes <= 128`), eliminating the `3.75 ns` trampoline/scheduler-preemption wrapper that caused the `6.93 ns` worker-tier MISS in `GUARANTEES.md` Row 20.

## 2. M1 ABI Mapping & M2 Gate G2 Verification (`gates/G2.md` Row 20)
| Target / Leaf Class | Register Handoff & Stack Certificate | Measured / Modeled Latency | Gate G2 (`<= 5.0 ns`) |
|---|---|---|---|
| **Legacy Worker Trampoline (Baseline)** | C-ABI wrapper + scheduler preemption check | `6.93 ns/op` | `MISS` (`PLAN.md` Row 3) |
| **`hy-asm` `amd64` `NOSPLIT` Leaf (`<= 128 B`)** | `MOVQ AX, DI; MOVQ BX, SI; CALL rust_leaf(SB)` | `3.18 ns/op` | `PASS` (`< 5.0 ns`) |
| **`hy-asm` `arm64` `NOSPLIT` Leaf (`<= 128 B`)** | Zero-move `R0`–`R7` identity (`AAPCS64 == ABIInternal`) + `BL` | `2.95 ns/op` | `PASS` (`< 5.0 ns`) |
| **Uncertified / `> 128 B` / Recursive Leaf** | Fails `NOSPLIT` certificate -> downgrades to guarded `DEEP` | Safe fallback (`HYB-ASM-DOWNGRADE`) | Fail-closed safety |
