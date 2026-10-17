# Pre-registration — Wildcard #375 Feasibility Spike (`spike19-certified-nosplit-leaves`)

- **Proposal:** [#375 — Kill `libhyrt.a` and the runtime stack-guard branch for SMALL/DEFAULT leaves: compile Rust leaves to Go-ABI0 assembly via LLVM + DWARF/stack-size certificates](https://github.com/limitlessprojects/Go-Hybrid/issues/375)
- **Author:** `@alejandrobrcca-sys`
- **Base commit:** `3119591d98650591e325bdb1d43c33cd3fbb1f06`

## Pre-registered Falsification Criteria (from #375)

1. **F1 — Automatic Stack-Bound & Call-Graph Classification Soundness:**
   - Given the machine-code frame sizes and relocation call graph extracted from `libhyrt.a` (`cargo build --release --manifest-path runtime/rust/hyrt/Cargo.toml`), a static call-graph verifier must:
     - Fail closed (`DEEP`) on any direct or mutual cycle (`serde_json` recursion, `regex_syntax` AST parser recursion, or `Value` drop glue).
     - Fail closed (`DEEP`) on any indirect call (`blr` / `call *reg`) whose target set is not statically closed.
     - Reject `NOSPLIT_DIRECT` admission for any function that calls opaque external C/OS symbols (`_pthread_mutex_lock`, `__rust_alloc`, `_CCRandomGenerateBytes`) whose stack consumption is invisible to `hyrt`'s object file.
     - Flag any discrepancy where the manual prose ledger in `interfaces/descriptors.md` under-estimates a function's true worst-case stack frame.
2. **F2 — Go `NOSPLIT` Linker & Trampoline Instruction Budget:**
   - Go's linker enforces a hard `StackNosplit` chain limit (~792 B on Go 1.26 amd64/arm64). Any leaf certified for direct `NOSPLIT` Go `.s` emission must satisfy $\text{max\_stack}(f) \le 512\text{ B}$ (leaving $\ge 280\text{ B}$ for the caller's `NOSPLIT` chain) with zero external/OS calls and zero unwind/panic landing pads.
   - For admitted leaves, compare the instruction path and shallow-goroutine (`Scenario A`, 8 KB fresh stack) behavior against `hybridge.Cross1` / `Cross1S`.
3. **F3 — Honest Wall Identification:**
   - Document which functions in the curated surface (`sha3binding`, `jsonbinding`, `regexbinding`, `hyrt` internals) pass `NOSPLIT_DIRECT` vs which hit hard architectural blockers (`StackNosplit` budget, `pthread_mutex_lock`, `serde_json::Value` drop glue in `table_insert`), without modifying any guarded compiler or runtime files.
