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
