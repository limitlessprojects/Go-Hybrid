# Borrowed lexer cursor — pre-registered comparison

Base: `b803f5556f4f75e8400bffe137f30717ae2e7831`.

Hypothesis: replacing the lexer-wide `Vec<char>` with a borrowed UTF-8 cursor
removes the full-source character copy without changing tokens, diagnostics,
or one-based character line/column positions. No grammar/API change is intended.
The token output still owns its strings. This is a normal memory/performance
improvement, with no new soundness/security finding.

Before changing the cursor, retain a release baseline executable built with the
same example driver. Generate the three bounded normal-source inputs with
`python3 spikes/lexer-borrowed-cursor/inputs.py NEW_DIRECTORY`.

Use `cargo build -p hy-syntax --example lexer_cursor --release --locked --offline`
from `compiler`. Run `lexer_cursor bench FILE 10` three times per input under
macOS `/usr/bin/time -l`; retain duration and maximum resident-set-size outputs.
Read input outside the timed loop. Every invocation is a fresh process; inputs
are already written, so this is not a cold-cache measurement. RSS includes the
process, input, and token output, not just the cursor allocation.

Compare complete `snapshot FILE` bytes between baseline and candidate for all
tracked `.hy` files and the three generated inputs. Also run existing syntax and
workspace/corpus/differential/example checks. Snapshot formatting is a same-Rust-
version comparison, not a public serialization format. No library dependencies
are added. Report any mismatches or material time regression; no speedup is
assumed in advance and a resource-only improvement must state its tradeoff.

## Observed results — 3 November 2026

Host: Apple M2 / macOS-26.5.2-arm64-arm-64bit-Mach-O, arm64,
Rust 1.99.0, release profile. Baseline `df0cae63` retains the original lexer;
candidate `eb432aa4` borrows `Chars`. Both measurement executables used the same
driver SHA256 `e7522be724ff6ef260946a10d269adcf5b53e33e740af7d200ff7516e1c3e264`.
The driver was subsequently rustfmt-formatted without a logic change.

Medians of three fresh-process observations; each process performs ten lexes:

| Input | Bytes | Baseline ms/lex | Candidate ms/lex | Baseline max RSS MiB | Candidate max RSS MiB |
| --- | ---: | ---: | ---: | ---: | ---: |
| ordinary | 27,808 | 0.353 | 0.370 | 2.23 | 2.14 |
| comments | 1,080,014 | 1.316 | 0.671 | 45.08 | 2.64 |
| unicode | 54,808 | 0.341 | 0.329 | 2.23 | 2.08 |

The comment-heavy repeated-lex case has lower observed process peak RSS and
time. The ordinary-source elapsed median is about 4.9% slower; Unicode-source
time is slightly lower. These short, single-machine observations do not
establish statistical significance or a universal speedup. Returned tokens and
strings still allocate: this removes the separate source-wide character vector.

Complete token/diagnostic Debug snapshots match byte-for-byte on all 175 tracked
`.hy` files plus three generated inputs. Two new public API tests pass for
Unicode block/line-comment lookahead, tab/character columns, a multiline raw
string and one EOF. No grammar, API or diagnostic change is intended.

[measurements.json](measurements.json) retains every timing/RSS observation and
input hash; [inputs.py](inputs.py) regenerates the bounded inputs. Build and keep
the example at both revisions; compare its `snapshot FILE` bytes for
`git ls-files '*.hy'` and the generated inputs. The fixed `input.hy` diagnostic
label avoids private path differences. Original full outputs are retained by
the contributor. Debug is compared on the same Rust version, not proposed as a
new stable serialization format.

Existing toolchain warning: rust-objcopy could not strip debug information
because its bundled libLLVM.dylib was unavailable. Cargo exited 0; both sides used
the same installed toolchain/profile, and original warnings/hashes are retained.

## Complete local validation

Tested code commit: `16353dbbd890614a3c50107f97aebb499b9766f1`, based on
`b803f5556f4f75e8400bffe137f30717ae2e7831`.

- Pinned offline `cargo test --workspace`: **2,631 passed**, zero failed/ignored.
- Fresh `cargo build -p hyb` passed.
- Checker corpus: **130/130** verdict agreement.
- Differential: **42/42** accepted build-and-run plus **88/88** rejected.
- All **ten examples** passed through the real compiler/Go pipeline.
- New Rust example and test formatting checks passed; `git diff --check` passed.

Verification uses the existing installed Rust 1.99.0 and Go 1.26.9 on macOS
arm64, cached dependencies and local scratch. The unrelated fixed-scratch runner
self-test was not run. No API/model, cloud job or package download was used.
The final evidence update preserves all three tested Rust files byte-for-byte.

Hosted CI, maintainer review, any award and payment remain unconfirmed.
AI-assisted implementation, measurement and test execution; no independent
human review is claimed.

## Identifier follow-up — 4 November 2026

A private call-site count profile on submitted head `2deb0be3f4136aa314aacd72b2870837c6831af7` found 12,912 of 56,835 ordinary-input lookaheads in identifier scanning (22.7%); the comment-heavy fixture had only 14 there. These are call frequencies, not CPU samples. A separate token count found 1,002 keyword tokens in the ordinary fixture.

The follow-up scans the existing ASCII identifier grammar over `Chars::as_str()`, keeps the prefix borrowed for case/context/keyword checks, and creates an owned string only for `Tok::Ident`. Consumed ASCII bytes are scalar columns; the first non-identifier byte stays at a UTF-8 boundary. Comment, string, numeric, keyword and diagnostic rules are unchanged. No dependency or whole-source character copy is added.

The first prefix-scanning candidate still created a String for keyword tokens and improved the ordinary median by 4.0%, below the locally recorded 5% selection target. It was refined rather than published. The final borrowed-keyword candidate is code `046dfa9a0ef37ad6f242faac2a164a00fa7e9df6`. Selection thresholds and the deterministic real-file choice were recorded locally before timing, not publicly committed in advance; this is an observed follow-up report, not a claim of public preregistration.

Same Apple M2/macOS arm64 host and Rust 1.99.0 release profile. Each case used five fresh alternating baseline/candidate process pairs. Generated cases used 2,000 lexes/process; the real-source case used 20,000 after the first version's roughly 25 ms process duration proved too short. The following values are medians of process-average ms/lex, compared with the submitted `2deb0be` head, not the original `Vec<char>` baseline:

| Input | Bytes | Submitted head ms/lex | Follow-up ms/lex | Change |
| --- | ---: | ---: | ---: | ---: |
| ordinary | 27,808 | 0.370888 | 0.339594 | -8.44% |
| comments | 1,080,014 | 0.689550 | 0.687894 | -0.24% |
| unicode | 54,808 | 0.321784 | 0.280643 | -12.79% |
| real_corpus | 5,364 | 0.012316 | 0.012220 | -0.78% |

The real case is `tests/checker-corpus/45-composite-literal-embeds-shared-map.hy`, the largest lex-valid tracked input among the existing snapshot set, chosen before timings. The small real/comment changes are reported without a significance claim. No universal speedup, new RSS, CPU-sample or allocation-counter result is claimed. Do not combine these ratios with the older cursor-only measurements to infer a newly measured result against the original baseline.

[All follow-up observations, input/source/binary hashes and method](identifier-followup-2026-11-04.json). Recreate the three generated files with the unchanged `inputs.py`; their regenerated hashes were checked against the measured files. Build and keep the existing `lexer_cursor` release example at both source revisions, then run `bench FILE 2000` (or `20000` for the real case) in five alternating pairs. Input read is outside the internal timer; token construction/drop is included. No new API/model, cloud job, package download or measured memory bound. Existing nonfatal strip/libLLVM warnings remain.

Full token/diagnostic snapshot bytes still match on all 175 tracked `.hy` files plus the three generated fixtures. Two additional public API regressions pin a long ASCII identifier before Unicode whitespace/operator positions and a long identifier ending at EOF; the position test file now has four passing tests.

Complete local validation at code `046dfa9a0ef37ad6f242faac2a164a00fa7e9df6`: **2,633 workspace tests passed**, zero failed/ignored; fresh compiler build; **130/130** checker verdicts; **42/42** accepted build-and-run plus **88/88** rejected differential cases; all **ten examples** passed. Tested source and test bytes are unchanged by this evidence update.

Whole-file `rustfmt --check` finds pre-existing differences elsewhere in the unchanged lexer baseline. The changed method is identical to pinned rustfmt output; the modified regression test file and existing example formatting checks pass. Those unrelated formatting differences are retained, not described as a full-file pass. Hosted verification for this follow-up, maintainer review, merge, award and payment remain pending.


## Punctuation allocation follow-up — 4 November 2026

Native CPU sampling of submitted head `e50bddcc` found ordinary-input time in punctuation prefix matching. `lex_punct` built a four-character `String` for every punctuation token. Current main still has that exact method. The two selected issue candidates already have claimants and were left alone.

Borrow `Chars::as_str()` and filter multi-operator candidates by their first ASCII byte before the existing longest-first prefix check. The operator table, fallback and scalar-position updates remain unchanged. Two guards pin overlapping operators at EOF and punctuation positions around multibyte whitespace.

The allocation experiment compares this patch against `e50bddcc`, with the same new `lexer_allocations` example in both trees. Observed successful `alloc`/`alloc_zeroed` calls per lex:

| Input | Before | After |
|---|---:|---:|
| Ordinary | 5,006 | 3,004 |
| Comments | 3 | 3 |
| Unicode | 3,006 | 2,004 |
| Fixed real source | 84 | 46 |

Reallocation counts did not increase. The ordinary call count fell about 40%; requested bytes fell from 826,163 to 810,147 per lex, about 2%. These are observed requests, not resident or peak-live memory. [Raw counts, timing runs and controls](punctuation-allocations-2026-11-04.json) retain the complete comparison.

The initial wall-time experiment did not pass its saved gate: ordinary median improved 28.0%, comments slowed 5.3%, Unicode improved 13.5% and the fixed real case improved 35.8%. An identical-binary A/A control differed 4.75%, failing the separately saved 2% resolution check for a 5% regression gate. Keep that failed/inconclusive timing result; it establishes neither a reliable new speedup nor a confirmed comment regression. Do not multiply these ratios by earlier measurements. A separate allocation-specific protocol was recorded before counting: at least 10% fewer ordinary allocation calls, no increases on the other cases, and exact/full correctness checks. Passing it does not validate the speed claim. Protocols were saved privately before their respective measurements and published afterward.

At tested code `7f1116d853ba4dbbcc9764219723bcf255fdd092`, all 178 complete token/diagnostic snapshots match. The 2,635 workspace tests include the two new punctuation guards; 130/130 corpus verdicts, 42/42 differential build-and-run cases, 88/88 rejects and all ten examples pass. New example and existing position-test files pass the pinned formatter; the two new test snippets pass. Both baseline and candidate punctuation methods retain the same pre-existing MULTI-table wrapping differences; no whole-lexer formatting pass is claimed. The final evidence update changes docs/status only and preserves the tested code bytes.

Reproduce observed request counts with Rust 1.99.0, from `compiler/`:

```sh
cargo run -p hy-syntax --example lexer_allocations --release --locked --offline -- FILE 10
```

For the baseline, create a separate checkout at `e50bddcc` and copy the same example into it before building with the same toolchain and flags. Inputs are the existing generated ordinary/comments/Unicode files plus the previously fixed real corpus source; generation and hashes are in the earlier spike. Input loading and output happen outside the counters; token construction and drops are included.

The [Rust GlobalAlloc reference](https://doc.rust-lang.org/std/alloc/trait.GlobalAlloc.html) warns that optimisers may elide/combine requests and allocator methods must avoid recursive allocation. This example forwards to System and updates only atomic counters inside allocator methods. Counters do not decide production correctness. Reference docs 1.99.1 are distinct from the actual 1.99.0 compiler.

One Mac and four fixtures: no reliable additional runtime-speed, RSS, universal allocation or other-platform claim. Hosted CI, maintainer review and any discretionary award/payment remain unconfirmed.
