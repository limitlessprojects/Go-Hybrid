# Error-alias emission follow-up to #3323 / #3325

Base: `a28ea1e371dc14b008b2b65d6dfbc23352f7a8bb`, 2 November 2026.
Local platform: macOS arm64; exact pins Rust 1.99.0 and Go 1.26.9.

After #3325, the checker accepts a T25 alias of builtin `error` in an E4
result, but the emitter still matches the literal name `error` when deciding
propagation returns and function/closure result arity. This patch reuses the
existing cycle-safe alias resolver for that result shape. It does not change
checker verdicts or introduce a new alias resolver.

## Executed controls

The integration test is `compiler/crates/hyb/tests/error_alias_emission_3323.rs`.
It checks parser/checker acceptance, emits Go, builds it using the pinned Go
compiler, runs the resulting executable, and checks exact stdout.

| Case | Unmodified base | Patched result |
| --- | --- | --- |
| Plain `(int, error)` propagation | Builds/runs: `0 sentinel` | Same |
| Caller returns `(int, failure)`, `failure = error` | Checker accepts; emission rejects `?` | Builds/runs: `0 sentinel` |
| Top-level callee returns `failure` | Checker accepts; Go assignment arity fails | Builds/runs: `callee` |
| Closure body returns `(int, deep_failure)` through alias chain | Checker accepts; emission rejects `?` | Builds/runs: `0 sentinel` |
| Called closure returns `failure` | Checker accepts; Go assignment arity fails | Builds/runs: `ok` |
| Negative control: `failure = int` | Rejects HYB-E2011 | Still rejects HYB-E2011 |

The new Rust regression file fails without the three emitter source changes:
1 test passes and 1 fails on the base. With the patch, both tests pass; the
accept-side test executes all five Go build/runtime cases.

## Required local verification

- `cargo check --workspace`: pass.
- `cargo test --workspace --quiet -- --test-threads=2`: **2509 passed, 0 failed,
  0 ignored**, including the new two tests.
- `python3 tools/corpus_harness.py --bin <built-hyb>`: **130/130** verdict
  agreement; managed tripwire not fired.
- `python3 tools/differential_loop.py --bin <built-hyb>`: **42/42** accepted
  programs built and ran; **88/88** rejection cases agreed.
- Existing quick Python checks, census, claims/guarantee links, registration,
  version consistency and conflict-marker guard: pass.

Cargo commands used `--offline --locked -j2`, a task-owned target/cache and the
pinned toolchains. The first differential pass reached 40/42 + 88/88 because
the separate runtime's exact locked dependencies were not yet cached. After
fetching those public packages without changing its lockfile, the full loop
passed offline. No test was removed, weakened or skipped.

The initial `make test-quick` stopped in the existing runner self-test recipe:
its unquoted `$(CURDIR)` splits a source path containing spaces. The remaining
constituent commands were completed directly with structured paths and an
exclusive task-owned self-test directory. The original Make invocation itself
is not reported as passing.

At original submission, hosted CI and maintainer review were pending. No award or payout is claimed.

## Upstream refresh — 3 November 2026

The original head `7f7949ffb5267ab0af8767a7442e72ea7254784c` completed
[hosted `ci` successfully](https://github.com/limitlessprojects/Go-Hybrid/actions/runs/41532490913):
both `compiler` and `quick` jobs passed. The separate
[shepherd-board run](https://github.com/limitlessprojects/Go-Hybrid/actions/runs/41532490480)
was cancelled; this is not an all-workflows-pass claim.

Main had advanced by 22 commits, including closure-result changes in all three
emitter files touched by this patch. A normal history-preserving merge through
`9aad9f84cad7681207df2530f2e7068be58168f0` passed workspace2522/2522,
corpus130/130, differential42/42+88/88, all ten examples and the documented
quick tooling constituents. Three more upstream commits arrived during that
validation, changing checker struct-literal validation. They were integrated
through `31e8806889a585093abd1003f0a5aebfd95550f0`; the emitter patch and the
original alias regression file remained byte-for-byte unchanged.

Final integrated-source checks with Rust 1.99.0 / Go 1.26.9 on macOS arm64:

- Alias integration file: 2/2 tests, including all five actual Go build/runtime
  cases and the non-error-alias rejection control.
- Full workspace: **2529/2529 passed, 0 failed, 0 ignored**.
- Corpus: **130/130**, managed tripwire not fired.
- Differential: **42/42** accepted build-and-run and **88/88** rejection cases.
- All **ten examples** passed. Unchanged quick tooling results from the first
  merge were retained; no live Kaggle/model job or manual Actions rerun was used.

The whole-workspace `cargo fmt --check` attempt reported differences in 192
files, 189 byte-identical to the pinned upstream tree. No formatter rewrites
were applied. The dedicated alias regression file passes its own rustfmt
check; this does not establish a repository-wide formatting pass. The existing
Makefile shared-temp self-test was not executed; its equivalent used a new
exclusive task-owned directory. No test was weakened, removed or skipped.

The PR-scope `git diff --check` against pinned upstream passes. The merge diff
against the prior parent exposes CRLF whitespace in the exact upstream
`issue_3405_struct_lit_soundness.rs` (43 lines); those upstream bytes were
preserved rather than mixed with unrelated line-ending edits.

Hosted CI for the refreshed head, maintainer review, merge and any discretionary
award or payout remain unconfirmed. Earlier hosted success belongs to the exact
original head identified above.
