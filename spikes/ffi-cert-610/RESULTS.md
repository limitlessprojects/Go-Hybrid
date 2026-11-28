# RESULTS — artifact-derived FFI boundary contracts (`#610`, W-026 class)

**Run against** `main` @ `7d63d5fc` (the branch's own base commit), pinned
`rustc 1.99.0`, flags `RUSTC_BOOTSTRAP=1 RUSTFLAGS="-Z emit-stack-sizes -C debuginfo=2"`.
Gates and fixtures were fixed in `PREREG.md` in commit `78e04dc`, **before**
any measurement; nothing below was measured before it.

## Verdict in one paragraph

The artifact gives more than the hand ledger has today — **all 21 exports get a
signature and a frame record straight out of `libhyrt.a`**, 1992 functions in
total, the contract is byte-reproducible across clean builds, and all six
pre-registered lies are rejected. But it gives **less** than a fast-path oracle:
under the pre-registered fail-closed policy **18 of 21 exports collapse to
`DEEP`** — not because a frame is big, but because the call closure of anything
that can allocate contains (a) a genuine recursion cycle through the
allocation-error path, (b) indirect dispatch, and (c) callees from the
**prebuilt `std`/`core`/`compiler_builtins` objects that carry no
`.stack_sizes` records at all**. So, today, a proof-carrying boundary can
**revoke** an optimization privilege but cannot **grant** one. That is the
`failure-with-knowledge` outcome the issue says is a valid, paid result — and
it is the answer to M1's "if the toolchain cannot prove a stack bound, the
honest successful result is DEEP".

## Re-verification — 2026-10-25, after the rebase

The PR was closed by the repo's 2026-10-24 queue reset (not a verdict on the
work) and the branch was rebased onto `main` @ `1f0ff97` — 72 commits later, no
conflicts, this spike's own diff unchanged (`spikes/ffi-cert-610/` only, no
compiler/runtime/hand-table edit). Everything below was re-run on that tree to
check that the published numbers still hold; the original run's text is left
untouched.

| step | on `1f0ff97` | vs published evidence |
|---|---|---|
| `build` (pinned flags) | artifact `d8dbd182…`, 346 members, 1992 functions with frames | **bit-identical artifact** |
| `extract` | 21 symbols; contract digest `f84a58…` on this base | see the digest note below — **content identical**, normalized digest reproduces `eb888898…` |
| `extract` ×3 | three clean extracts are byte-identical to each other | determinism holds on the new base too |
| `verify` | exit **1**, **16** diagnostics, **0** gaps, coverage `21/21` signatures, `19` consumed by Go, `36` call sites | **file identical** to `evidence/g1-verify.json` |
| `fixtures` | **6/6** rejected | **file identical** to `evidence/fixtures.json` |
| `report` | artifact table | **file identical** to `evidence/table.md` |
| `closure_analysis.py` | why the closure leaves the provable set | **file identical** to `evidence/closure-analysis.txt` |
| `test_hy_ffi_cert.py` | 14 tests | **OK** |
| G4 — Go bridge in the CI-shaped module | `go vet ./...`, `go build ./...`, `go test ./...` green (hybridge, jsonbinding, regexbinding, sha3binding, hybench, overheadbench); both `TestDeepNesting*` pass; `gofmt -l runtime/go` clean | same |

**Note on the contract digest (G3).** `contract_sha256` is a digest over the
whole contract object, and that object records `artifact.base_commit` — the
commit the run happened on. So a re-run on a new base legitimately produces a
different digest (`f84a58…` here) while nothing about the artifact or the
symbol records changed. Proven rather than asserted: replacing `base_commit`
with `78e04dc` in the new contract and re-digesting gives
**`eb8888981e5950248fd476ce8404ef36551c4e099d068e8847728e878bf13292`** — the
published value, exactly. Per-symbol records are equal, the artifact is
bit-identical, and three repeated extracts on this base are byte-identical to
each other. If the digest is meant to travel between checkouts, `base_commit`
is the one field that has to leave the canonical form (or be an explicit
input); that is a design remark for whoever implements the `abi-types.md` §5
handshake, not a change to this PR.

**G5 cost, spot-checked:** one clean build 17 s, one extract 6 s → 6/17 =
35%, against the published 34.2% from medians of three. Not re-measured with
medians here; the number is a single-run sanity check, and the "most of it is
`objdump`/`readelf` over 346 members" explanation is unchanged.

**Has `main` fixed or absorbed this in the meantime?** Not yet: `#610` is
still absent from `MOONSHOT.md` (the ledger still ends at W-023), so the
proposal is still not triaged into the track, and the question in the issue
(whether a third-party spike earns the M2/ledger half) is still unanswered.
What *did* change is the money rail: `BOUNTY.md` is now a plan of record and
Track 3 spells out the wildcard payout — `$250` when the proposal is accepted
into the ledger, `$250` when its learning is published, "success or failure
alike". This spike's outcome (fail-closed certificate that can revoke a
privilege but not grant one, with all six falsifiers rejected) is exactly the
`failure-with-knowledge` shape that second half names.

Raw capture of this section: `evidence/reverify-2026-10-25.txt`.

## What was run

```
# 1. artifact, exactly the pinned flags
RUSTC_BOOTSTRAP=1 RUSTFLAGS="-Z emit-stack-sizes -C debuginfo=2" \
  cargo build --release --manifest-path runtime/rust/hyrt/Cargo.toml

# 2. contract
python3 spikes/ffi-cert-610/hy_ffi_cert.py extract --out spikes/ffi-cert-610/evidence/contract.json

# 3. claims vs facts (exit 1 = rejected)
python3 spikes/ffi-cert-610/hy_ffi_cert.py verify  --contract spikes/ffi-cert-610/evidence/contract.json

# 4. the six pre-registered negative fixtures
python3 spikes/ffi-cert-610/hy_ffi_cert.py fixtures --contract spikes/ffi-cert-610/evidence/contract.json \
  --out spikes/ffi-cert-610/evidence/fixtures.json

# 5. why the closure leaves the provable set
python3 spikes/ffi-cert-610/closure_analysis.py
```

Note: `PREREG.md` says `results/`; the directory is `evidence/` because the
repository's `.gitignore` reserves `results/` for non-committed job logs
("commit summaries only, not bulk logs"). No content changed with the rename.

Raw captures: `evidence/contract.json`, `evidence/g1-verify.json`,
`evidence/fixtures.json`, `evidence/closure-analysis.txt`, `evidence/table.md`,
`evidence/g3-determinism.txt`, `evidence/g5-timing.txt`, `evidence/toolchain.txt`.

## Gates

| # | Gate | Result | Evidence |
|---|------|--------|----------|
| **G1** | Signature/arity coverage | **split**: 21/21 exports have a DWARF-derived signature (arity + per-parameter ABI class); of the 19 symbols the Go bindings consume, **16/19 match exactly**, 3 are the *zero-arity dummy-word* convention (below) | `evidence/g1-verify.json`, `evidence/table.md` |
| **G2** | 6/6 negative fixtures rejected | **PASS — 6/6** | `evidence/fixtures.json` |
| **G3** | Deterministic contract bytes | **PASS** — three clean builds produced `artifact sha256 d8dbd182…` and `contract_sha256 eb888898…`, symbols identical | `evidence/g3-determinism.txt` |
| **G4** | No added runtime cost | **PASS** — the tool is build-time only (nothing links into `libhyrt.a`: it is a Python program reading the archive, not a crate); the existing bridge tests pass unchanged, including both deep-nesting tests | §"G4" below |
| **G5** | Generation cost (measurement) | **34.2%** of a clean build (6.11 s vs 17.83 s, medians of 3). Above the 10% *measurement target*; reported as a number, not scored as pass/fail — per PREREG | `evidence/g5-timing.txt` |

### G1, honestly

21/21 export signatures and 100% arity coverage — that is the half the issue
asked for and it works. The three mismatches are not compiler defects; they are
a **missing concept in the trampoline family**: there is no `Cross0`, so a
zero-arity export (`hyrt_abi_word`, `hyrt_stack_alignment_probe`) can only be
called through `Cross1*` with a dummy word, and a guards test does the same for
`hy_nop`. A contract that wants to be exact must model a `dummy_arg`
convention for zero-arity exports; a checker that ignores it reports false
drift (as the first version of this one did).

### G4

```
$ cd <assembled GOPATH-shaped module> && CGO_ENABLED=1 go test -count=1 ./hybridge ./sha3binding ./regexbinding ./jsonbinding
ok  	hybrid.program/hybridge	0.006s
ok  	hybrid.program/sha3binding	0.006s
ok  	hybrid.program/regexbinding	0.014s
ok  	hybrid.program/jsonbinding	0.006s

$ go test -count=1 -run TestDeepNesting -v ./jsonbinding
--- PASS: TestDeepNestingParsesViaDeepPath (0.00s)
--- PASS: TestDeepNestingOnConstrainedStack (0.00s)
```

The staticlib used by those tests is a plain `cargo build --release` (no spike
flags), so the pass is not an artefact of the measurement build.

## The artifact table

`python3 spikes/ffi-cert-610/hy_ffi_cert.py report --contract …`

| export | arity | own frame B | closure B | artifact class | forced by | Go route | signature | route |
|---|---|---|---|---|---|---|---|---|
| `hy_json_array_get` | 3 | 136 | 176 | DEEP | recursion, indirect-dispatch, missing-frame x25, external-callee x4 | DEEP | match | match |
| `hy_json_array_len` | 2 | 40 | 64 | DEEP | recursion, indirect-dispatch, missing-frame x5, external-callee x4 | SMALL | match | **too fast** |
| `hy_json_array_push_string` | 3 | 88 | 128 | DEEP | recursion, indirect-dispatch, missing-frame x19, external-callee x6 | SMALL | match | **too fast** |
| `hy_json_get_array` | 4 | 120 | 784 | DEEP | recursion, indirect-dispatch, missing-frame x23, external-callee x5 | DEEP | match | match |
| `hy_json_get_int` | 4 | 88 | 752 | DEEP | recursion, indirect-dispatch, missing-frame x12, external-callee x4 | SMALL | match | **too fast** |
| `hy_json_get_string` | 4 | 104 | 768 | DEEP | recursion, indirect-dispatch, missing-frame x13, external-callee x5 | SMALL | match | **too fast** |
| `hy_json_live_count` | 1 | 40 | 48 | DEEP | recursion, indirect-dispatch, missing-frame x7, external-callee x5 | — | not consumed | — |
| `hy_json_new_array` | 1 | 56 | 80 | DEEP | recursion, indirect-dispatch, missing-frame x6, external-callee x5 | SMALL | match | **too fast** |
| `hy_json_new_object` | 1 | 56 | 80 | DEEP | recursion, indirect-dispatch, missing-frame x5, external-callee x4 | SMALL | match | **too fast** |
| `hy_json_parse` | 3 | 200 | 224 | DEEP | recursion, indirect-dispatch, missing-frame x26, external-callee x7 | DEEP | match | match |
| `hy_json_release` | 1 | 40 | 64 | DEEP | recursion, indirect-dispatch, missing-frame x6, external-callee x4 | DEEP | match | match |
| `hy_json_set` | 4 | 184 | 208 | DEEP | recursion, indirect-dispatch, missing-frame x18, external-callee x5 | DEEP | match | match |
| `hy_json_to_string_pretty` | 4 | 136 | 176 | DEEP | recursion, indirect-dispatch, missing-frame x20, external-callee x6 | DEEP | match | match |
| `hy_json_value_text` | 4 | 120 | 144 | DEEP | recursion, indirect-dispatch, missing-frame x5, external-callee x4 | — | not consumed | — |
| `hy_nop` | 1 | 0 | 0 | SMALL | — | RAW | drift 2→1 | match |
| `hy_regex_is_match` | 4 | 248 | 272 | DEEP | recursion, indirect-dispatch, missing-frame x26, external-callee x7 | DEEP | match | match |
| `hy_regex_is_valid` | 2 | 40 | 64 | DEEP | recursion, indirect-dispatch, missing-frame x5, external-callee x4 | DEEP | match | match |
| `hy_sha3_digest256` | 3 | 776 | 800 | DEEP | recursion, indirect-dispatch, missing-frame x6, external-callee x5 | SMALL | match | **too fast** |
| `hy_sha3_stream_checksum` | 2 | 408 | 408 | DEEP | recursion, indirect-dispatch, missing-frame x6, external-callee x4 | SMALL | match | **too fast** |
| `hyrt_abi_word` | 0 | 0 | 0 | SMALL | — | DEFAULT | drift 1→0 | match |
| `hyrt_stack_alignment_probe` | 0 | 0 | 0 | SMALL | — | DEEP | drift 2→0 | match |

`closure B` is the worst-case sum along the deepest resolvable direct-call path
(cycles contribute 0, which is exactly why recursion is escalated instead of
being summed). "too fast" means: **the artifact alone cannot certify the route
the binding takes** — it does not mean a frame overflow was observed. The
argument that these are safe today is the prose in `interfaces/descriptors.md`,
i.e. a human assertion, which is precisely what the proposal wants to replace.

## Why 18/21 are DEEP — three facts, measured

1. **The prebuilt `std`/`core`/`compiler_builtins` objects carry no stack
   records.** 4824 functions in the object-code call graph, 1992 with a
   `.stack_sizes` record:

   ```
   1227  std-44675f186029a4cd
    538  compiler_builtins-2ae2df7d631cccc2
    534  core-667b96598095cbda
    136  gimli-ccadb29741f100ae
    112  alloc-a8d2bee804de7f49
   ```

   Only the crates compiled from source in this build (`hyrt`, `sha3`,
   `serde_json`, `regex`) have records. `-Z emit-stack-sizes` cannot reach
   `libstd` as shipped, so any closure that allocates, formats or panics has an
   unrecorded callee — and unrecorded means `DEEP` under the policy.

2. **The allocation-error path is genuinely recursive in the artifact**, so
   "no recursion" is unprovable for anything that can allocate:

   ```
   __rust_dealloc → __rdl_alloc → CString::new/spec_new_impl
                  → raw_vec::handle_error → __rust_dealloc
   ```

3. **Calls are GOT-indirected.** On this toolchain nearly every cross-function
   call is `call *0x0(%rip)` with an `R_X86_64_GOTPCREL` relocation naming the
   callee; jump tables and static slots appear as data-symbol relocations and
   are therefore true indirect dispatch. The tool keeps two readings:
   *linked* (follow the GOT edge to the named symbol — the table above) and
   *strict* (any GOT edge ⇒ `DEEP`, interposition-safe). Under **strict, all 21
   exports are `DEEP`**, including `hy_nop`.

## The six fixtures

| fixture | lie | verdict |
|---|---|---|
| `f1_signature_drift` | 3-param export claimed as 2 | `HYB-E7001` ✔ |
| `f2_recursion_as_small` | recursive/over-guard export claimed `SMALL` | `HYB-E7002` ✔ |
| `f3_retained_borrow` | `may_retain_ptr=false` over a wrapper with `Box::leak` + `Box::into_raw` + `ptr::write` | `HYB-E7003` ✔ |
| `f4_callback_edge` | `may_callback=false` while the closure has indirect dispatch | `HYB-E7004` ✔ |
| `f5_blocking_beneath_nonblocking` | `non_blocking=true` while the closure reaches `Mutex` | `HYB-E7005` ✔ |
| `f6_hash_mismatch` | artifact embeds a digest of a different contract | `HYB-E7006` ✔ |

**6/6 rejected.** The pre-registered prediction that F3 would be decidable only
from the wrapper source (not the artifact) held: F1, F2, F4, F5, F6 are
artifact-decidable; F3 is not.

## What is derivable, and what is not

| Contract field | Source | Status |
|---|---|---|
| exported symbol set | `.symtab` of every archive member | **proven** (21/21) |
| arity + per-parameter ABI class | DWARF `DW_TAG_subprogram` / `formal_parameter` (`-C debuginfo=2`) | **proven** (21/21) |
| own frame size | `.stack_sizes` (`-Z emit-stack-sizes`) | **proven** for source-built crates, **absent** for prebuilt `std` |
| worst-case stack class | call graph + records + fail-closed escalation | **computable but currently always `DEEP`** — see above |
| recursion / indirect dispatch / external callee | object code | **proven** |
| blocking | callee symbol names (`Mutex`, `sleep`, `std::fs` …) | **hint only** — a name is not a proof |
| may-retain / may-callback | restricted wrapper subset (source) | **assertion**, never proven by the artifact |
| binding freshness | canonical digest embedded in the artifact | **proven** (deterministic, and a stale digit is rejected) |

## Consequences for the proposal (what M1 actually shows)

- **The certificate direction is the useful one.** A proof-carrying contract can
  say "this export may not take the fast path" with the artifact as the source
  of truth — that works today, on the whole surface, with a reproducible digest.
  It cannot say "this export may take the fast path" until the *whole* closure
  has records.
- **The smallest policy that prevents a hand assertion from granting a
  privilege** is: claims may only ever be *weaker* than artifact evidence; a
  faster route requires either (a) a from-source `std` build with stack records
  (`-Zbuild-std` + `emit-stack-sizes`, **not tested here**), or (b) an explicit
  per-export assertion that is invalidated by any change to the artifact digest
  and is visible in the ledger as an assertion, not as evidence.
- **The duplicate tables are replaceable in one direction.** Generated
  signatures/records can drive `hy-check`/`hy-emit` tables
  (`foreign.rs`, `shapes.rs`) and `interfaces/descriptors.md`, because those
  are exactly the fields the artifact proves. The *effect* fields
  (retain/callback/blocking) must stay assertions with a downgrade default —
  `handles/DEEP/blocking` — or the certificate just re-encodes the same trust
  in a new file.
- **Contract bytes are deterministic**, so the hash handshake in
  `abi-types.md` §5 is implementable without a canonicalization layer.

## What this does NOT show

- It does not show that the eight `SMALL` routes overflowed. Nothing here
  measured a stack overflow; it measured **what the artifact can prove**.
- It does not run on `ff569022` (the commit the issue cites). It runs on
  current `main`; `runtime/rust/hyrt/src/lib.rs` has 21 exports there, which is
  consistent with the surface the issue describes.
- `-Zbuild-std` as the fix is **unverified here** — stated as the obvious next
  experiment, not as a result.
- The effect scan is syntactic; it is a restricted subset, not dataflow, and it
  is honest about that in `PREREG.md` §5.

## Cost

Generation 6.11 s median vs 17.83 s median clean build = **34.2%**. Most of it
is `objdump`/`readelf` over 346 archive members; per-member caching by
(content hash, flags) would cut it to seconds on an incremental build — not
implemented, so not claimed.

## Reproduction

```
make -C runtime/rust/hyrt ...            # not wired into the Makefile by this spike
python3 spikes/ffi-cert-610/hy_ffi_cert.py build
python3 spikes/ffi-cert-610/hy_ffi_cert.py extract --out /tmp/c.json
python3 spikes/ffi-cert-610/hy_ffi_cert.py verify  --contract /tmp/c.json
python3 spikes/ffi-cert-610/hy_ffi_cert.py fixtures --contract /tmp/c.json
python3 spikes/ffi-cert-610/hy_ffi_cert.py report  --contract /tmp/c.json
python3 spikes/ffi-cert-610/closure_analysis.py
python3 -m unittest spikes/ffi-cert-610/test_hy_ffi_cert.py     # from the repo root
```
