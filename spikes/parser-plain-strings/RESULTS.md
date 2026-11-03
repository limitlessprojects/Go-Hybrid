# Ordinary-string parser allocation spike

## Pre-registered before implementation and measurement

Base: `77b97fa4fa2c4c33ecffbdff6c183505e8a3d333`.

The parser clones the first cooked string into an interpolation-parts vector
even when the next token is `StrEnd`; it then discards that vector. Hypothesis:
returning the same literal and span immediately for `StrChunk StrEnd` avoids
one payload clone and the unused parts allocation. Other token sequences must
keep their existing path and diagnostics.

References: SPEC-LANGUAGE LX7/T4; lexer deferred StrEnd emission; parser string
handling, nested-quote tests, spans and speculative array-type rollback; current
PLAN/STATUS, contribution requirements and BOUNTY useful-improvement rules.
Moving token payloads was rejected because rollback can revisit them. This
candidate keeps tokens intact.

Validation plan:

1. Run the allocation regression on the original parser first. A10007-byte
   literal should show the redundant payload-sized copy; its value must survive.
2. Build an identical dependency-free release example before and after the
   small early return; retain exact binaries, source and input hashes.
3. Compare AST/diagnostic debug output byte-for-byte over tracked .hy files and
   focused empty/escaped/nested/malformed string inputs.
4. Five deterministic parse-only workloads: short/long/plain Unicode+escapes,
   interpolation and numeric controls. Allocation/reallocation calls/requested
   bytes in a separate untimed parse;20timed complete parses/process and three
   alternating baseline/candidate process pairs/case. Both use an instrumented
   System allocator. Counts are not RSS or end-to-end compiler performance.
5. Retain only with semantic parity and expected allocation reduction. A
   material control slowdown requires diagnosis/rejection. Report sample spread
   and environment; no statistical or universal speed claim.
6. If retained, run workspace/corpus/differential/examples. Hosted CI, maintainer
   review and any discretionary reward remain separate.

No result yet. No new dependency, service/model call, account, financial setup
or expense. The published merit pool is not an award or payment promise.


## Measured parse-only outcome

Rust 1.99.0, release opt-level 2, Darwin 25.5.0 arm64. The same instrumented
System allocator and benchmark helper were used on both versions. Each case
has three alternating baseline/candidate process pairs, 20 timed complete parses
per process. Allocation/reallocation counts and requested bytes are collected
in a separate untimed parse.

| Workload | Before ms/parse | After ms/parse | Median time change | Alloc/realloc calls | Requested bytes saved/parse |
| --- | ---: | ---: | ---: | ---: | ---: |
| interpolation-control | 1.2425 | 1.2601 | +1.42% | 29941 → 29941 | 0 |
| numeric-control | 1.1865 | 1.1638 | -1.91% | 26040 → 26040 | 0 |
| plain-long | 1.7295 | 1.7113 | -1.05% | 2465 → 2209 | 581,632 |
| plain-short | 1.1381 | 1.0244 | -9.98% | 24040 → 20040 | 956,000 |
| plain-unicode-escapes | 0.5642 | 0.5151 | -8.70% | 13028 → 11028 | 465,890 |

Counts are stable in all three samples per version/case. Nonempty plain-string
samples avoid exactly two allocation calls per literal. Empty strings retain
their meaning but are not claimed to save these allocations. The interpolation
control is 1.42% slower in the measured sample; the numeric control is 1.91% faster
without executing this path. Those controls show why timing is a local sample,
not a universal or statistically established speedup. Allocation-request totals
are not peak RSS, live-memory savings or end-to-end compiler measurements.

The payload-copy regression fails on the unchanged parser: 2 payload-sized
allocations versus the allowed 1. It passes after the nine-line early return.
All 169 initial AST/diagnostic snapshots match byte-for-byte (165 parse successes,
4 errors): existing corpus/examples/ownership-spike files, 13 focused strings,
and 5 generated workloads. Token contents, fallback interpolation logic, span
calculation and grammar/checker rules are unchanged.

### Review against references

1. **Semantics before speed:** moving tokens would break speculative rollback.
   Keep tokens intact and return only for the already-complete StrChunk/StrEnd
   form. LX7/T4, lexer closing-token emission, nested quotations and 169 snapshots
   support the narrower approach.
2. **Measure the claimed resource:** the defect is a discarded parts allocation
   and payload clone, so count actual allocator requests rather than infer them
   from RSS. The original regression fails and the candidate passes.
3. **Expose tradeoffs:** report all five workloads, raw paired samples and the
   small interpolation-control slowdown. No compiler-wide or hardware-wide claim.
4. **Preserve scope:** full parser formatting already differs from rustfmt on
   the base. The added block is formatted, the same legacy differences remain,
   and no unrelated code is reformatted.
5. **Delivery/reward:** complete required local checks, then await hosted CI and
   maintainer review. No merge, merit rank or payment is inferred from the data.

### Reproduce the narrow comparison

Use baseline commit `7a897a96a0452d3841d10e2e0443c11ce5eae5fe` and code
candidate `43ce8334fe3e8e28460a1e47f4f7418e3a2beab7` in separate worktrees.
Both contain the same example and generated workloads. With the pinned toolchain:

```sh
python3 spikes/parser-plain-strings/inputs.py /tmp/parser-string-inputs
cargo build --manifest-path compiler/Cargo.toml -p hy-syntax --example parser_strings --release --locked --offline
# Run the built release example (adjust the path if CARGO_TARGET_DIR is set):
compiler/target/release/examples/parser_strings bench /tmp/parser-string-inputs/plain-short.hy 20
compiler/target/release/examples/parser_strings snapshot /tmp/parser-string-inputs/plain-short.hy
```

Repeat in alternating before/after order for all five generated inputs. The
raw measurements include source hashes, counts, ranges and individual samples.
Offline builds require an already-populated dependency/toolchain cache.

## Required local validation

On code candidate `43ce8334fe3e8e28460a1e47f4f7418e3a2beab7`:

- Workspace: **2,655 passed, 0 failed, 0 ignored**.
- Checker corpus: **130/130**.
- Differential: **42/42 accepted build-and-run, 88/88 rejected**.
- All **ten examples** pass with pinned Go 1.26.9 / Rust 1.99.0.
- Test registration, conflict-marker and corpus-census guards pass.
- Added Rust files pass rustfmt; the existing parser-wide formatting deviations
  are unchanged and the new block matches rustfmt output.

These are local observations on one Mac. Hosted CI, merge, merit classification,
award and payment remain unconfirmed. AI-prepared code and evidence, with no
independent human review claimed.
