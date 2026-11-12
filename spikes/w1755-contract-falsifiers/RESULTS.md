# #1755: check the ABI and permission contracts before a rewrite

Source proposal: <https://github.com/limitlessprojects/Go-Hybrid/issues/1755>.
This is an independent feasibility audit, not the implementation in PR #1756.
It does not change the checker, runtime, or Wildcard ledger, or claim acceptance.

## Pre-registration

Record this plan in Git before executing the experiments. Use the repository's
pinned Go 1.26.9 on native linux/arm64. No Rust crate benchmark is planned.

| Probe | Question and fixed decision rule | Control / limit |
|---|---|---|
| A | Can an external package define a Plan 9 `TEXT` function with `<ABIInternal>`? If the pinned assembler rejects the ABI selector, the ordinary `.s` + `//go:noescape` route does not implement the proposed direct ABIInternal boundary. | Build and execute a stack-argument ABI0 identity function in the same environment. Force a function-value call and inspect the resulting executable for the ABI wrapper and its call to `.abi0`. A patched compiler remains a separate possible design. |
| S | Does a per-function frame threshold of 128 bytes imply a whole-call-path bound below 192 bytes? Assemble and execute a three-function NOSPLIT call chain, each declaring 96 local bytes. Its simultaneously live local frames alone require at least 288 bytes, before frame overhead. | This is a bounded, nonrecursive Go-assembly example, not a measurement of any Rust kernel or a stack-overflow experiment. Observe the actual linked instructions. The counterexample applies to per-function maxima, not a certificate that already includes callees. |
| C | Does the literal 16-bit permission split `mask >> 1` conserve a full mask? Evaluate both OR reunion and integer addition of two equal shifted children from `0xffff`. Also divide exact rational fractions as a control. | The proposal leaves reunion underspecified. A counterexample to the literal arithmetic does not disprove linear capabilities, a different split rule, or H1 corpus parity. No replacement checker is implemented here. |

The experiments succeed as an audit only if the positive executable control
runs, the expected negative is checked against the actual assembler diagnostic,
and raw commands, exit statuses, output, and toolchain versions are retained.
An unexpected result or a missing tool is a failure of this audit run, not a
license to replace it with modeled output. Do not report timings or extrapolate
to amd64, Rust crate stack bounds, corpus equivalence, GC safety, or H3 latency.

## Results

Executed on 2026-11-10, native Linux ARM64, `go version go1.26.9 linux/arm64`.
Base: `7cf29833bfcdc68619fa4d43269a2d03fb24cfc8`.
Pre-registration commit: `6c2ee2f` (before the runner or fixtures were executed).
Tested source commit: `3e0b482`.

| Probe | Actual observation | Decision |
|---|---|---|
| A | `go tool asm -p w1755probe` exits 1 with the ABI-selector restriction. The ABI0 control builds, executes four identity inputs (including zero and maximum `uint64`), and passes `go vet`. A forced function-value call retains `main.identity`, whose linked instructions call `main.identity.abi0`. | The ordinary external `.s` route does not directly select ABIInternal. `//go:noescape` is present in the control and does not remove its ABI wrapper. This does not rule out a compiler/intrinsic change. |
| S | The chain executes and returns. All three functions retain their calls/frames in linked disassembly: each allocates **112 bytes**, for **336 simultaneously live bytes** at the leaf. No `runtime.morestack` call appears in these three functions. Each individual frame is still below 128 bytes. | A maximum individual frame below 128 bytes does not prove a whole-call-path bound below 192 bytes, even without recursion or dynamic allocation. A genuine leaf or a transitive certificate remains a possible narrowing. |
| C | `0xffff >> 1 = 0x7fff`. Two equal children overlap in `0x7fff`; OR reunion is `0x7fff`, integer-sum reunion is `0xfffe`. Exact fractions `1/2 + 1/2` recover `1`. | The literal shifted-child rule needs a defined conserving split/reunion operation. This is an arithmetic specification gap, not a disproof of capability systems or an H1 corpus result. |

Raw observations, not modeled estimates:

- [Toolchain version](evidence/go-version.stdout.txt),
  [assembler diagnostic](evidence/external-abiinternal.stderr.txt), and
  [positive execution](evidence/execute-control.stdout.txt).
- [ABI wrapper disassembly](evidence/wrapper.stdout.txt) and
  [nested frame disassembly](evidence/chain.stdout.txt).
- [Permission arithmetic](evidence/permissions.json) and
  [machine-readable summary with source hashes](evidence/summary.json).

The runner's final exit status was 0; the negative assembler invocation's
expected exit 1 is checked by diagnostic, not mistaken for an audit failure.
It retains each command and both output streams in the chosen output directory.
The compact committed evidence excludes the executable and bulk symbol list.
Committed disassembly text has only trailing whitespace removed; the runner
retains the original stdout bytes as text in its output directory.

## Local validation

- Pinned Rust 1.99.0: `cargo test --workspace --locked`, **3,285 passed**, zero
  failed or ignored; `cargo build -p hyb --locked` passed.
- `python3 tools/corpus_harness.py --bin <hyb>`: **139/139** verdict agreement.
- `python3 tools/differential_loop.py --bin <hyb>`: **47/47** accepted programs
  built and executed, **92/92** expected rejections, full agreement.
- This spike's native runner: exit 0, including `go vet` on the control and
  the specifically matched expected assembler rejection.

The first Rust test build ran out of disk before executing the suite. The
successful repeat disabled incremental compilation, stripped test symbols,
and used `RUSTFLAGS=-C prefer-dynamic` with the pinned Rust standard library
on `LD_LIBRARY_PATH`. No source or test expectation changed.
The first differential runs hit VCS-stamping errors caused by an unrelated
invalid `/tmp/.git`; the passing run used a task-owned ignored directory inside
the valid clone as `TMPDIR`, with the compiler's original Go build flags.
The failed attempts and successful repeats are retained in the worker's test
log. Hosted CI remains pending and is required before final review.

## Learning and next decision

The baseline matters: [the current descriptor ledger](../../interfaces/descriptors.md)
already defines a guarded fast path on the caller's goroutine stack. Eliminating
a stack switch on a path which already avoids one is not a measured saving.
The proposal's register-preservation criterion also needs per-architecture
contracts: the pinned Go ABI assigns the goroutine register to **R28 on ARM64**,
versus R14 on amd64.

Before deleting either production mechanism, separate the design choices:

1. Keep ordinary Go assembly and account for ABI0 adaptation, or explicitly
   budget a compiler/intrinsic integration for direct ABIInternal calls. This
   experiment establishes a build constraint, not the cost of either choice.
2. Make a stack certificate cover the maximum simultaneously live path,
   including frame overhead and callees; reject unknown/recursive paths or
   prove their bounds. A per-function `.stack_sizes` entry alone is insufficient.
   Removing a runtime headroom check additionally needs a proof of available
   headroom at the caller. `NOSPLIT` itself is not that proof.
3. Define capability creation, split, reunion, consumption, and branch/loop
   joins before claiming corpus parity. The equal right-shift interpretation
   tested here can be replaced; any replacement still needs conservation and
   alias-lifetime tests against the real checker corpus.

These are scoped learnings for review of #1755. **H1 corpus parity, actual Rust
kernel stack certificates, GC safety, and H3 latency remain untested.** No
performance number, Rust-crate support, accepted milestone, or new guarantee is
claimed. The existing pending PR #1756 was reviewed as competition metadata;
its implementation and evidence were not used for this audit.

## Reproduction

```sh
python3 spikes/w1755-contract-falsifiers/run.py --output /tmp/w1755-new-run
```

Requires a new output directory, native Linux ARM64, Python 3.9+, and Go 1.26.9.
The separate, path-filtered workflow runs this same command with a verified
native Go archive. Hosted CI and maintainer review are pending.
