# Issue #2771: array lengths in operand compatibility

Base: `92eba51cd984a3251beda5b86c2faadc889d54ef`.
Toolchains: Rust 1.99.0 and Go 1.26.9, Linux x86_64. Compilation used
`CARGO_BUILD_JOBS=2` and `flock /tmp/heavy.lock`.

## Reproduction

The initial 13-test regression set had 10 failures and 3 passing controls on the
unchanged checker. It accepted different array lengths in equality, assignment,
typed declarations, arguments, returns, nested arrays, aliases, literals and
constant-expression lengths. These were verdict failures, not parser failures.
An independent Go 1.26.9 build rejects the `[3]int == [4]int` comparison.

## Change

`OpKind::Array` retains an optional folded integer length alongside the element
kind. AST types use the existing source-position-aware constant evaluator;
array literals use the existing length resolver, including inferred lengths.
Generic type-argument strings retain literal lengths. Lengths that cannot be
resolved remain unknown, rather than being treated as zero or mismatches.

Assignment/equality and strict copy/conversion checks compare known lengths
recursively. Conversion compatibility preserves all pre-existing non-length
distinctions, including function results and arity, while tolerating unresolved
array lengths nested in otherwise matching types. Indexing, slicing, iteration
and addressability retain their existing behavior.

No parser, emitter, dependency or lockfile changes were needed.

## Validation

- Final focused suite: **20 passed, 0 failed**.
- Final compiler workspace: **2383 passed, 0 failed, 0 ignored**, 270 suite results.
- Checker corpus: **130/130**, managed tripwire not fired; no corpus verdict changed.
- Original Issue examples through the real `hyb check --json` CLI: rejection exit
  1, `HYB-E1002`, and diagnostics showing `[3]int` versus `[4]int`.
- Matching inferred/declared-length control through `hyb build` with Go 1.26.9:
  built and executed successfully, printing `true`.
- Test registration, conflict-marker guard, corpus census and `git diff --check`:
  passed. The new Rust integration test file passes rustfmt.
- Final differential loop: **42/42 accept-side build-and-run; 88/88 reject-side**,
  FULL AGREEMENT. The earlier offline run could not build the Rust runtime for
  two unchanged corpus cases because `regex` was not cached. After fetching the
  runtime's locked dependencies, the full compiler workspace, corpus and
  differential commands were rerun successfully against `fec085c`.
- This final documentation update leaves the tested source, integration tests,
  manifests and lockfiles unchanged. Hosted CI and maintainer review remain
  pending.

The environment's installed Go 1.27.1 was not used for the successful final
workspace/CLI run: it is rejected by the repository's toolchain pin. The pinned
Go 1.26.9 toolchain was selected through `GOTOOLCHAIN`, without changing project
pins or test expectations. Existing upstream compiler warnings remain.

## Refresh onto current main

Rebased onto `8b3efb21c5a1504bb0af4c5ead6d650237803250` after upstream
advanced. The new upstream array-addressability checks added two one-field
`OpKind::Array` patterns; both now ignore the additional length field. Their
behavior is preserved. The original two patches rebase unchanged (range-diff).

Fresh validation on `c251974fbc8d40179a8911f5a997a345a018079d`:

- Focused regressions/controls: 20/20.
- Full compiler workspace: 2412 passed, no failures or ignored tests.
- Corpus: 130/130; differential FULL AGREEMENT, 42/42 accept + 88/88 reject.
- Registration, census, conflict markers and diff guards pass.
- Original comparison and assignment examples reject through the CLI with exit
  1 and HYB-E1002 showing `[3]int` and `[4]int`; valid control builds/runs `true`.

The assistant sandbox places a protected, non-repository `.git` placeholder
under `/tmp`. The first refreshed differential attempt therefore failed Go VCS
stamping for two bridge modules, with exit 128 from Git. Setting `TMPDIR` to
the repository's ignored `target/verification-tmp` lets those temporary modules
use the real repository for VCS stamping; the complete battery then passes.
No compiler, Go toolchain, test expectation, or VCS-stamping setting was altered
for this environment correction. Source/tests/lockfiles remain unchanged by
the subsequent evidence-only update. Hosted CI and review remain pending.

## Repeat

From the repository root, with the pinned tools on PATH:

```bash
export RUSTUP_TOOLCHAIN=1.99.0
export GOTOOLCHAIN=go1.26.9
export CARGO_BUILD_JOBS=2
mkdir -p target/verification-tmp
export TMPDIR="$PWD/target/verification-tmp"
flock /tmp/heavy.lock cargo test --locked --manifest-path compiler/Cargo.toml -p hy-check --test array_length_compatibility_2771
flock /tmp/heavy.lock cargo test --locked --manifest-path compiler/Cargo.toml --workspace -- --test-threads=2
flock /tmp/heavy.lock cargo build --locked --manifest-path compiler/Cargo.toml -p hyb
python3 tools/corpus_harness.py --bin compiler/target/debug/hyb
python3 tools/differential_loop.py --bin compiler/target/debug/hyb
```

If using `CARGO_TARGET_DIR`, adjust the two `--bin` paths to that directory.
