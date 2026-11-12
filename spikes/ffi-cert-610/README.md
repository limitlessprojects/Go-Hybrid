# ffi-cert-610 — artifact-derived FFI boundary contracts

Spike for [#610](https://github.com/limitlessprojects/Go-Hybrid/issues/610):
*"Make FFI claims proof-carrying: verify boundary contracts against the linked
Rust artifact"*.

The question this spike answers is deliberately narrow:

> **Which parts of a boundary contract can be derived from the linked
> artifact itself, and what must remain a human assertion — and if a human
> assertion is wrong, does the build fail closed?**

`PREREG.md` fixes the gates, the toolchain and six negative fixtures **before**
any measurement. `RESULTS.md` reports what actually happened, including the
result that does not flatter the idea.

## Read in this order

1. [`PREREG.md`](PREREG.md) — gates G1–G5, fixtures F1–F6, the restricted
   wrapper subset, and the disclosures (committed first: `78e04dc`).
2. [`RESULTS.md`](RESULTS.md) — the measured answer, the artifact table, and
   the three facts that make 18 of 21 exports `DEEP`.

## What is in here

| file | what it is |
|---|---|
| `hy_ffi_cert.py` | the tool: ELF/archive reader, `.stack_sizes` reader, object-code call graph, DWARF signature reader, Go-binding reader, contract builder, verifier, fixture runner, markdown report |
| `closure_analysis.py` | the "why does everything escalate" analysis (missing frame records by archive member, the recursion cycle, per-export forcing facts) |
| `test_hy_ffi_cert.py` | unit tests for the parsers, the canonical digest, the wrapper subset, and the fixtures |
| `evidence/` | raw captured output of every number quoted in `RESULTS.md` |

## Quick start

```sh
# from the repository root
python3 spikes/ffi-cert-610/hy_ffi_cert.py build
python3 spikes/ffi-cert-610/hy_ffi_cert.py extract  --out /tmp/c.json
python3 spikes/ffi-cert-610/hy_ffi_cert.py verify   --contract /tmp/c.json   # exit 1 = a claim is not proven
python3 spikes/ffi-cert-610/hy_ffi_cert.py fixtures --contract /tmp/c.json   # 6/6 must be rejected
python3 spikes/ffi-cert-610/hy_ffi_cert.py report   --contract /tmp/c.json   # artifact facts vs Go routes
python3 spikes/ffi-cert-610/closure_analysis.py
python3 -m unittest spikes/ffi-cert-610/test_hy_ffi_cert.py
```

Requirements: a Rust toolchain whose `rustc` accepts
`-Z emit-stack-sizes` (reached through `RUSTC_BOOTSTRAP=1`), `binutils`
(`nm`/`objdump`/`readelf`/`objcopy`/`ar`), Python 3.9+. No Python packages.

## Design notes worth knowing before reading the code

- **Signature facts come from DWARF, not from source.** `-C debuginfo=2` emits
  `DW_TAG_formal_parameter` DIEs; `-C debuginfo=1` does not. That is why the
  build command pins level 2.
- **`.stack_sizes` layout on this toolchain** is one section per function
  section: an 8-byte relocated address slot plus a ULEB128 size, with the
  relocation pointing at the function's *section symbol*. `stack_records()`
  documents that and resolves names through the symbol table.
- **Two readings of a GOT call.** `call *0x0(%rip)` + `R_X86_64_GOTPCREL` names
  the callee but is interposable. The `linked` reading follows the edge; the
  `strict` reading forces `DEEP`. Both are reported; neither is silently
  mixed into the other.
- **Fail-closed escalation** (recursion, indirect dispatch, missing record,
  unknown external callee) forces `DEEP` — the tool never manufactures a
  number to keep a fast path alive.
- The diagnostic codes `HYB-E7001…E7006` are local to this spike. They are not
  a request for a diagnostic allocation; the project may assign its own.

## Scope

No compiler change, no runtime change, no edit to the hand tables. This spike
measures whether the tables *can* be generated and where they cannot be. It
does not delete `interfaces/descriptors.md`, `hy-check/src/foreign.rs` or
`hy-emit/src/shapes.rs`.
