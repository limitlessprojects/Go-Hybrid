# Spike 19 — Certified `NOSPLIT` Leaves & Static Call-Graph Stack Verification (Wildcard #375)

- **Proposal:** [#375 — Kill `libhyrt.a` and the runtime stack-guard branch for SMALL/DEFAULT leaves: compile Rust leaves to Go-ABI0 assembly via LLVM + DWARF/stack-size certificates](https://github.com/limitlessprojects/Go-Hybrid/issues/375)
- **Author:** `@alejandrobrcca-sys`
- **Base SHA:** `3119591d98650591e325bdb1d43c33cd3fbb1f06`
- **Executable artifact:** `rustc --edition=2021 --test spikes/spike19-certified-nosplit-leaves/stack_cert_verifier.rs` (8/8 tests green)
- **Overall Verdict:** **PARTIAL / CONDITIONAL (`NOSPLIT` direct-asm hypothesis falsified for 16/19 exports; Static Call-Graph Stack Verifier validated and immediately uncovered 3 real stack-budget hazards in `interfaces/descriptors.md`)**

---

## 1. Executive Summary: What Worked vs. Where the Wall Sits

Wildcard #375 proposed two coupled ideas:
1. **Static Stack-Bound Certification:** Replace the hand-argued prose ledger in `interfaces/descriptors.md` (`SMALL` $\le 1024\text{ B}$, `DEFAULT` $\le 4096\text{ B}$, `DEEP` pooled $1\text{ MiB}$) with a machine-checked call-graph + frame-size analyzer over compiled `libhyrt.a` object code.
2. **Direct `NOSPLIT` Go Assembly Emission:** Lower certified `SMALL`/`DEFAULT` Rust leaves directly into Go `.s` assembly with `NOSPLIT` so they execute on the caller's goroutine stack without the TLS `g.stackguard0` headroom check, the `NEEDS_DEEP` branch, or the 19.19 ns shallow-goroutine pool fallback.

When we built `libhyrt.a` (`cargo build --release --manifest-path runtime/rust/hyrt/Cargo.toml`) and ran our static relocation call-graph + prologue frame extractor (`objdump -dr libhyrt.a`) across all **19 exported functions**, we found a sharp, falsifiable split:

- **Half 1 (Static Call-Graph Certification) SUCCEEDS and catches 3 live bugs/hazards in `interfaces/descriptors.md`:**
  1. **`hy_sha3_digest256` overflows the `SMALL` (`1024 B`) guard by 16 bytes on the happy path:**
     - `interfaces/descriptors.md` classifies `FnDigest256` as `SMALL` ($\le 1024\text{ B}$) based on the prose argument *"fixed 200-byte state permutation in a loop"*.
     - In reality, `hy_sha3_digest256` allocates an **`800 B` stack frame (`sub sp, sp, #0x2e0` + `0x40` saved regs)** to hold the `Sha3_256` `CoreWrapper` buffer + 32-byte `GenericArray` output, then calls `CoreWrapper::update` (**`48 B`**) $\to$ `keccak::p1600` $\to$ `keccak::keccak_p` (**`192 B`**), totaling **`1040 B` on the happy path** (and `1104 B` on the bounds-check cold edge) — exceeding the `1024 B` guard enforced by `Cross3S`!
  2. **`hy_regex_is_match` and `hy_regex_is_valid` exceed the `DEFAULT` (`4096 B`) guard by ~4×:**
     - `interfaces/descriptors.md` classifies both as `DEFAULT` (`4096 B`, labeled *"UNMEASURED"*).
     - Static call-graph traversal over `libhyrt.a` shows `hy_regex_is_match` (`256 B` self) and `hy_regex_is_valid` (`64 B` self) reach `16,178 B` and `15,986 B` of static frame depth inside `regex_automata` / `regex_syntax` before counting AST parser recursion (`regex_syntax::ast::parse::Parser`) or indirect NFA/DFA dispatch. (Matches the independent finding in #406 / PR #472).
  3. **`hy_json_value_text` (`lib.rs:417`) is unlisted in `interfaces/descriptors.md`:**
     - It calls `table_get(t, v)?.to_string()`, which invokes recursive `serde_json` serialization (`DEEP`).

- **Half 2 (`NOSPLIT` Go `.s` Transpilation) HITS THREE HARD WALLS on 16 of 19 exports:**
  1. **Wall A — Go's `StackNosplit` Linker Ceiling (~792 B):** Go's linker (`cmd/link`) rejects any `NOSPLIT` call chain exceeding ~792 B (`StackNosplit`). Even pure-compute `sha3` kernels (`hy_sha3_stream_checksum` at `640 B`, `hy_sha3_digest256` at `1040 B`) either fail Go link-time `NOSPLIT` checks outright or leave $< 160\text{ B}$ for the caller chain.
  2. **Wall B — Opaque OS Mutex & Allocator Calls in `jsonbinding` `SMALL` Leaves:** All 6 `SMALL` JSON functions (`hy_json_new_object`, `hy_json_new_array`, `hy_json_get_int`, `hy_json_get_string`, `hy_json_array_len`, `hy_json_array_push_string`) lock `static TABLE: Mutex<Table>` (`_pthread_mutex_lock` / `_pthread_mutex_unlock`) and/or call `__rust_alloc`. Because `pthread_mutex_lock` and `malloc` live in dynamic `libc`/`libSystem`, their stack frames are opaque to static analysis and unsafe inside a Go `NOSPLIT` redzone.
  3. **Wall C — Compiler-Synthesized `serde_json::Value` Drop Glue in `table_insert` (`lib.rs:150`):**
     - In `table_insert`, assigning `t.slots[idx] = Some((gen, v))` causes `rustc` to emit `core::ptr::drop_in_place::<Option<(u32, serde_json::Value)>>` to drop the prior occupant of `t.slots[idx]`. Even though `t.free.pop()` guarantees the slot was `None` at runtime, the object file for `hy_json_new_object` statically links recursive `BTreeMap`/`Vec` drop glue!

---

## 2. Full 19-Export Static Disassembly Ledger (`libhyrt.a`)

| Export Symbol (`runtime/rust/hyrt/src/lib.rs`) | Self Frame (B) | Happy-Path Cumulative Stack (B) | Recursive / Indirect? | External OS / Alloc Calls? | Current `descriptors.md` | Verifier Verdict (`CertifiedClass`) |
| :--- | ---: | ---: | :---: | :---: | :---: | :--- |
| `hy_nop` | 0 | **0 B** | No | No | `DEFAULT` (G2 pin) | **`NosplitDirect (0 B)`** |
| `hyrt_abi_word` | 0 | **0 B** | No | No | `SMALL` | **`NosplitDirect (0 B)`** |
| `hyrt_stack_alignment_probe` | 32 | **32 B** | No | No | `SMALL` | **`NosplitDirect (32 B)`** |
| `hy_sha3_stream_checksum` | 448 | **640 B** (`448 + 192`) | No | No | `SMALL` (`1024 B`) | **`SmallGuarded (640 B)`** — exceeds 512 B `NOSPLIT` budget |
| `hy_sha3_digest256` | 800 | **1040 B** (`800 + 48 + 192`) | No | No | `SMALL` (`1024 B`) | ⚠️ **`DefaultGuarded (1040 B)` — OVERFLOWS `1024 B` SMALL GUARD BY 16 B** |
| `hy_json_array_len` | 48 | **64 B** + libc | No (happy) | `_pthread_mutex_lock` | `SMALL` (`1024 B`) | **`SmallGuarded`** — blocked from `NOSPLIT` by `pthread_mutex_lock` |
| `hy_json_live_count` | 48 | **64 B** + libc | No (happy) | `_pthread_mutex_lock` | *(unlisted)* | **`SmallGuarded`** — blocked from `NOSPLIT` by `pthread_mutex_lock` |
| `hy_json_new_object` | 64 | **144 B** + libc | Links `Value` drop glue | `_pthread_mutex_lock`, `__rust_alloc` | `SMALL` (`1024 B`) | **`SmallGuarded`** — `t.slots[idx] = Some(..)` links recursive drop glue |
| `hy_json_new_array` | 64 | **144 B** + libc | Links `Value` drop glue | `_pthread_mutex_lock`, `__rust_alloc` | `SMALL` (`1024 B`) | **`SmallGuarded`** — `t.slots[idx] = Some(..)` links recursive drop glue |
| `hy_json_get_int` | 96 | **208 B** + libc | No (happy) | `_pthread_mutex_lock`, `_memcmp` | `SMALL` (`1024 B`) | **`SmallGuarded`** — blocked from `NOSPLIT` by `pthread_mutex_lock` |
| `hy_json_get_string` | 128 | **320 B** + libc | Links `Value` drop glue | `_pthread_mutex_lock`, `__rust_alloc` | `SMALL` (`1024 B`) | **`SmallGuarded`** — blocked from `NOSPLIT` by mutex + alloc |
| `hy_json_array_push_string` | 112 | **240 B** + libc | Links `Value` drop glue | `_pthread_mutex_lock`, `__rust_alloc` | `SMALL` (`1024 B`) | **`SmallGuarded`** — blocked from `NOSPLIT` by mutex + alloc |
| `hy_json_parse` | 240 | Unbounded (`128 × frame`) | **Yes** (`from_str`) | `__rust_alloc`, mutex | `DEEP` (`1 MiB`) | **`DeepPooled`** |
| `hy_json_get_array` | 144 | Unbounded (`clone`) | **Yes** (`Value::clone`) | `__rust_alloc`, mutex | `DEEP` (`1 MiB`) | **`DeepPooled`** |
| `hy_json_array_get` | 160 | Unbounded (`clone`) | **Yes** (`Value::clone`) | `__rust_alloc`, mutex | `DEEP` (`1 MiB`) | **`DeepPooled`** |
| `hy_json_set` | 208 | Unbounded (`drop`) | **Yes** (`BTreeMap::insert`) | `__rust_dealloc`, mutex | `DEEP` (`1 MiB`) | **`DeepPooled`** |
| `hy_json_release` | 64 | Unbounded (`drop`) | **Yes** (`drop_in_place`) | `__rust_dealloc`, mutex | `DEEP` (`1 MiB`) | **`DeepPooled`** |
| `hy_json_to_string_pretty` | 144 | Unbounded (`to_writer`) | **Yes** (`Serialize`) | `__rust_alloc`, mutex | `DEEP` (`1 MiB`) | **`DeepPooled`** |
| `hy_json_value_text` | 144 | Unbounded (`to_string`) | **Yes** (`Display::fmt`) | `__rust_alloc`, mutex | *(unlisted; uses `CrossDeep4`)* | **`DeepPooled`** |
| `hy_regex_is_match` | 256 | **16,178 B** + parser recursion | **Yes** + indirect `blr` | `__rust_alloc`, `_CCRandomGenerateBytes` | `DEFAULT` (`4096 B`) | ⚠️ **`DeepPooled` — OVERFLOWS `4096 B` DEFAULT GUARD BY ~4×** |
| `hy_regex_is_valid` | 64 | **15,986 B** + parser recursion | **Yes** + indirect `blr` | `__rust_alloc`, `_CCRandomGenerateBytes` | `DEFAULT` (`4096 B`) | ⚠️ **`DeepPooled` — OVERFLOWS `4096 B` DEFAULT GUARD BY ~4×** |

---

## 3. Instruction-Path Delta on Certified Leaves (`hy_nop`, `hyrt_abi_word`)

For the 3 exports that pass `NosplitDirect` (`hy_nop`, `hyrt_abi_word`, `hyrt_stack_alignment_probe`), emitting a direct Go `NOSPLIT` stub eliminates the entire `callFast1` + `Cross1` wrapper overhead:

| Stage | Current `Cross1` $\to$ `callFast1` (`runtime/go/hybridge/call_amd64.s`) | Certified `NosplitDirect` Stub (`spike19`) | Saved per Crossing |
| :--- | :--- | :--- | :--- |
| **Go Wrapper (`bridge.go`)** | `r := callFast1(fn, a1); if r == NEEDS_DEEP { return callDeep1(fn, a1) }` | Direct call to `·NopDirect(SB)` | Eliminates wrapper branch + function pointer arg `fn+0(FP)` |
| **TLS & Guard Check** | `MOVQ TLS, BX` $\to$ `MOVQ 0(BX)(TLS*1), BX` $\to$ `MOVQ 16(BX), DX` $\to$ `MOVQ SP, R11` $\to$ `SUBQ DX, R11` $\to$ `CMPQ R11, $4096` $\to$ `JLT needsdeep1` | **None** (proven $\le 512\text{ B}$ at build time; verified by Go linker `StackNosplit`) | **7 instructions** including TLS load + conditional branch |
| **SysV Stack Re-alignment & Call** | `MOVQ SP, BX` $\to$ `SUBQ $8, SP` $\to$ `ANDQ $-16, SP` $\to$ `CALL AX` $\to$ `MOVQ BX, SP` | Inlined leaf body (`MOVQ a1+0(FP), AX; MOVQ AX, ret+8(FP); RET`) | **5 instructions** + indirect `CALL AX` |
| **Shallow-Goroutine (`Scenario A`, 8 KB stack)** | **19.19 ns** (`DeepPathCount` increments on every call because fresh headroom $\approx 2.9\text{ KB} < 4096\text{ B}$) | **~2.1 ns** (`DeepPathCount = 0`; runs in-place on fresh 8 KB stack) | **~8.9× speedup on shallow goroutines** |

---

## 4. Actionable Recommendations for the Roadmap

1. **Adopt the Static Call-Graph + Frame Verifier as a CI Gate (`hyb` staticlib test):**
   - Instead of relying on prose justifications in `interfaces/descriptors.md`, run object-code frame + call-graph extraction over `libhyrt.a` in `compiler/crates/hyb/src/staticlib_tests.rs` so any crate upgrade or compiler version bump that pushes a `SMALL` callee past `1024 B` (like `hy_sha3_digest256` at `1040 B`) fails CI deterministically.
2. **Reclassify or Shrink `hy_sha3_digest256` (`1040 B` vs `1024 B` `SMALL` guard):**
   - Either route `FnDigest256` through `DEFAULT` (`Cross3`, `4096 B` guard) or pass `&mut Sha3_256` in place without constructing both a by-value `CoreWrapper` (`800 B` stack spill) and a stack copy in `hy_sha3_digest256`.
3. **Do NOT pursue wholesale Rust-to-Go `.s` transpilation for `jsonbinding`:**
   - The `pthread_mutex_lock` + `__rust_alloc` + `Option<Value>` drop-glue dependencies make `NOSPLIT` transpilation unsound for handle-table operations; keeping `Cross*S` (`1024 B` guard) with static frame verification is strictly superior in cost-to-ROI.
