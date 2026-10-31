# PREREG — proof-carrying FFI boundary contracts, artifact-derived (`spike/ffi-cert-610`)

**Status: pre-registration. Committed before any measurement was taken.**
Anything measured after this commit is reported in `RESULTS.md` as-is; gates
below are not allowed to move.

Target: issue [#610](https://github.com/limitlessprojects/Go-Hybrid/issues/610)
— *"[wildcard] Make FFI claims proof-carrying: verify boundary contracts
against the linked Rust artifact"*.
Base commit under test: `7d63d5fc25ceb525c2d0f6ec89a4ad608a6a5011` (current
`main`, 2026-10-23). The issue cites `ff569022`; see §6 for the disclosure.

This spike is an **independent** implementation. It does not reuse, read or
depend on the withdrawn `spikes/w026-ffi-boundary-cert/` from PR #1114
(closed by its own author); the design choices here are made from the
issue text and the repository's own files.

---

## 1. The question and the deliverable

The boundary currently trusts three hand-maintained copies of the same facts
(`interfaces/descriptors.md`, `compiler/crates/hy-check/src/foreign.rs`,
`compiler/crates/hy-emit/src/shapes.rs`; the Rust surface is hand-curated per
`runtime/rust/hyrt/src/lib.rs:15-18`). The claim of a fast path is only as
good as its *worst-case stack upper bound*; the ledger is that argument, and
it is prose.

This spike asks one falsifiable question:

> **Which parts of a boundary contract can be derived from the linked
> artifact itself, and what must remain a human assertion — and if a human
> assertion turns out to be wrong, does the build fail closed?**

The deliverable is a build-time tool, `hy-ffi-cert`, that

1. extracts the exported C ABI and per-function stack records **from the
   built `libhyrt.a`** (not from source),
2. closes the stack records over the **direct call graph read out of the
   object code**, forcing `DEEP` on recursion, indirect calls, missing
   records and unknown external callees,
3. cross-checks every symbol the Go bindings consume for **signature/arity**
   and **route class**, and
4. hashes the resulting contract canonically and can bind that hash into the
   artifact, so a stale binding cannot certify a different binary.

M1 scope only (certificate parity + fail-closed falsification). M2's
"run it on sha3, serde_json, regex" is exactly what this artifact already
contains, so the same run covers it; no crate-level migration is attempted.

## 2. Pinned toolchain and exact commands

Recorded in `results/toolchain.txt` by `build.sh`; repeated here so the
gates are fixed:

```
rustc 1.99.0 (b940084d7 2026-09-28)
cargo 1.99.0 (5f94df478 2026-08-27)
RUSTC_BOOTSTRAP=1
RUSTFLAGS="-Z emit-stack-sizes -C debuginfo=2"
cargo build --release            # in runtime/rust/hyrt/ (crate-type = staticlib,
                                 #  panic = "abort", codegen-units = 1, opt-level = 3)
objcopy --add-section .hy_ffi_cert=contract.json \
        --set-section-flags .hy_ffi_cert=noload,readonly libhyrt.a
```

`-Z emit-stack-sizes` is unstable and is reached through `RUSTC_BOOTSTRAP=1`.
**If that stops working on a later toolchain the tool must fail closed
(`DEEP`), never silently skip the gate** — that is itself one of the
pre-registered behaviours.

## 3. Gates (fixed now, pass/fail, no post-hoc movement)

| # | Gate | Pass criterion |
|---|------|----------------|
| **G1** | Signature/arity coverage | 100% of exported `hy_*` / `hyrt_*` symbols in the artifact have a DWARF-derived signature (arity + per-parameter ABI class), and 100% of symbols consumed by the Go bindings (`extern` declarations and `hybridge.Cross*` call sites) match that signature. Counted, both directions. |
| **G2** | Fail-closed over six negative fixtures | 6/6 fixtures rejected or conservatively reclassified to `DEEP` before an unsafe route is selected. Zero fixtures may pass. |
| **G3** | Deterministic contract bytes | Two clean builds (`cargo clean`) of the pinned toolchain produce a byte-identical contract JSON and identical SHA-256, and identical per-symbol stack records. |
| **G4** | No added runtime cost on a validated fast path | The tool is build-time only. Evidence: (a) the contract is not consulted at run time — no symbol from the tool is linked into `libhyrt.a`; (b) the repository's existing bridge and deep-nesting Go tests still pass unchanged. If (b) cannot run in this sandbox, the gate is reported **not measured**, with the reason, and is not scored as a pass. |
| **G5** | Generation cost (measurement, not a correctness gate) | Wall time of certificate generation as a percentage of a clean `cargo build --release` of `runtime/rust/hyrt`, measured three times, median reported. **<10% is a "pass", ≥10% is reported as a number, not as a failure** — the issue says measurement, not requirement. |

Every gate gets a row in `RESULTS.md` with the actual command and its actual
output. A gate that cannot be measured is written `not measured` and counts
as a failure of the spike's *claim*, never as a pass.

## 4. The six pre-registered negative fixtures

Each fixture takes the **real artifact facts** and a **lying or stale claim**,
and must be rejected (or downgraded) before a route faster than the facts
allows can be selected. Fixed code names:

| Fixture | Lie | Detection required | Expected verdict |
|---|---|---|---|
| **F1** `signature_drift` | Contract/binding claims 2 parameters for an export the artifact says takes 3 (or: arity drift between the `extern` declaration and the `Cross<N>` call site) | Artifact DWARF signature vs Go `extern` vs call-site arity | `REJECT E7001` |
| **F2** `recursion_as_small` | An export whose artifact call closure is recursive (or whose measured worst-case stack exceeds the guard) is claimed `SMALL` | `.stack_sizes` + call-graph closure | `REJECT E7002` |
| **F3** `retained_borrow` | `may_retain_ptr=false` claimed for an export whose restricted-wrapper body contains a retention primitive | Restricted-wrapper source scan (see §5) | `REJECT E7003` |
| **F4** `callback_edge` | `may_callback=false` claimed for an export whose artifact closure contains an indirect call / a callback-taking callee | Object-code closure | `REJECT E7004` |
| **F5** `blocking_beneath_nonblocking` | `non_blocking=true` claimed for an export whose artifact closure reaches a blocking primitive (mutex lock / sleep / file I/O symbols) | Object-code closure | `REJECT E7005` |
| **F6** `hash_mismatch` | Artifact carries an embedded contract section whose digest does not match the regenerated contract | Read the section back out of the artifact, recompute | `REJECT E7006` |

**Pre-registered prediction (falsifiable by our own run):** we expect F1,
F2, F4, F5, F6 to be decidable from the artifact, and **F3 to be decidable
only from the wrapper source**, i.e. the artifact cannot prove
"never retains a pointer". If that prediction is wrong in either direction,
`RESULTS.md` says so and it changes the M2 conclusion.

## 5. Restricted wrapper subset (the honest ceiling)

Effect facts that ordinary type checking cannot prove — never retains,
never calls back, never blocks — are accepted **only** for a deliberately
restricted wrapper body. Pre-registered subset rules (all syntactic,
conservative, no dataflow):

- Allowed pointer crossings: raw pointers may only be converted with
  `slice::from_raw_parts` / `from_raw_parts_mut` and used for the duration of
  the call, or copied into a heap object owned by the callee (handles).
- A wrapper body that contains any of `Box::into_raw`, `Box::leak`,
  `mem::forget`, `ManuallyDrop`, `ptr::write`, a `static mut`, or a store of a
  `*const`/`*mut` into a struct field or static → **may_retain = true**
  (downgrade to handles/copies, never pinned borrows).
- A wrapper body that stores or takes an `extern "C" fn` / `fn` pointer it
  does not call immediately within the same expression → **may_callback =
  true** (reject in safe mode unless reclassified).
- A wrapper body that reaches `Mutex::lock`, `RwLock`, `thread::sleep`,
  `std::fs`, `std::net`, `std::io::stdin` → **blocking = true**.
- Anything the subset cannot classify → the strongest conservative field
  (`DEEP`, blocking, retain, callback), never the fast one.

A claim stronger than the subset's verdict is rejected; a claim equal or
weaker is accepted. This is the "smallest policy that prevents a human
assertion from granting an unsafe privilege".

## 6. Disclosures

- **Exploratory probes before this commit** (disclosed, as in the repo's own
  spike protocol): (a) `cargo build --release` of `runtime/rust/hyrt`
  succeeds in 15.4 s and produces a 27 MB `libhyrt.a`; (b) `-Z emit-stack-sizes`
  with `RUSTC_BOOTSTRAP=1` produces `.stack_sizes` sections in the members;
  (c) `-C debuginfo=2` yields `DW_TAG_formal_parameter` DIEs, `-C debuginfo=1`
  does not. No gate in §3 was measured before this commit, and no fixture was
  run before it.
- **Commit under test.** The issue cites `ff569022ee5b40b685ec0f96f9f358ee26044e9b`;
  this spike runs against current `main` `7d63d5fc` because the pinned
  toolchain, `Cargo.lock` and the `runtime/` layout are what a maintainer
  would build today. Divergence in `runtime/rust/hyrt/src/lib.rs` between the
  two commits is reported in `RESULTS.md` if it exists.
- **AI assistance.** Produced with disclosed AI assistance; every number in
  `RESULTS.md` comes from a command whose output is captured in `results/`.
- **Codes.** `E7001–E7006` are local to this spike. They are not a request
  for a diagnostic allocation; if the project wants these checks, the codes
  are its to assign.
- **Scope honesty.** This is not a compiler change and it does not delete the
  hand tables. It measures whether they can be generated, and where they
  cannot be.

## 7. What would falsify the spike's own thesis

If, after running, any of the following holds, the thesis "the boundary
contract can be derived from the artifact" is **false as stated** and
`RESULTS.md` will say so plainly:

- `.stack_sizes` records are missing for reachable callees of the pinned
  crates, so that every export collapses to `DEEP` (then stack certification
  is not yet usable — a real, publishable negative result);
- the DWARF signature of an export contradicts its Go `extern` declaration
  on the real, shipped surface (then the shipped binding is unverified today);
- the contract is not byte-reproducible across clean builds (then the hash
  handshake cannot be trusted without a canonicalization step the tool cannot
  perform);
- any of the six fixtures can still select a fast route.

## 8. Amendment 1 (2026-10-25, after the rebase, before the re-run)

The reported run was on `main` @ `7d63d5fc`. The repo then ran a queue reset and
this PR was closed as not planned; the branch was rebased onto `main` @
`1f0ff97` (72 commits later, no conflicts, this spike's own diff unchanged) and
the whole sequence was re-run.

**Nothing is renegotiated.** The gates in §3, the six fixtures in §4, the wrapper
subset in §5 and the toolchain pin are exactly as registered; `RESULTS.md` keeps
the first run's numbers and reports the re-run separately. The commit under test
moves from `7d63d5f` to `1f0ff97` for the same reason §6 gives for running on
current `main` rather than the commit the issue cites — but note that
`contract_sha256` digests the artifact's `base_commit` field, so the digest is
expected to differ on a new base even when the artifact and the symbol records
are identical. `RESULTS.md` §"Re-verification" proves the equivalence rather
than asserting it (normalizing `base_commit` back to `78e04dc` reproduces the
published `eb888898…`).
