# #1086: pre-registered dominance-only falsifiers

Task: https://github.com/limitlessprojects/Go-Hybrid/issues/1086

Base: `cc59624818a7e2298e3b7d977762600192cbaf77` (main).

## Scope and thresholds (registered before execution)

This independent companion experiment tests the proposal's strongest P1
claim: single assignment and ordinary CFG dominance alone can replace
ownership back-edge/alias checks. It does not implement a Hybrid frontend,
replace Tier-0, claim corpus equivalence for a new checker, or award itself
a ledger entry. No production compiler behavior will change.

- F1: a single-definition, dominance-valid loop that consumes the same
  resource on two iterations is a counterexample. A bounded interpreter must
  report the second consumption; a fresh allocation on every iteration is
  the safe control. A structural dominance rejection of either graph does
  not count as a counterexample.
- F2: a dominance-valid alias copy followed by a move and an alias write is
  a counterexample. A deep-copy control must stay independent. This tests
  storage identity, not just the spelling of lexical identifiers.
- F3: a dominance-valid SSA map update with a live old map version must
  preserve the old version to count as functional. A local Go test will
  compare an in-place helper against a copying helper. Observing the update
  through the old version falsifies in-place map_put as a functional update.
- V1: include negative controls for malformed SSA (non-dominating use,
  duplicate definitions, incorrect phi predecessor sets, and invalid edge
  operands). The verifier must reject these rather than accept everything.
- V2: run the unchanged compiler workspace, full checker corpus, and
  differential harness with the repository's pinned toolchains. Record
  actual counts/exits; these are baseline regressions, not proof that this
  experiment is a replacement compiler. Run the experiment in CI too.

P1 is falsified if any of F1/F2 passes structural SSA verification while
its bounded execution violates ownership. This is a necessary-condition
experiment: one counterexample suffices; it cannot establish the converse.
If P1 fails, do not claim P2 or full historical-corpus parity. The Go map
probe is a semantic check, not an escape-analysis or performance measurement.

## Results

Not run at pre-registration (commit `4941229`, committed before executing
the experiment). The thresholds above are unchanged.

### Executed 2026-11-27, Linux aarch64

Python 3.12.3, Rust/Cargo 1.99.0, Go 1.26.9. Main advanced during validation
to `67ba97f5078408f56edcb403f7b6dcd3db2a06c3`; that update changes only one
upstream heartbeat-log line and was merged locally. Compiler, runtime,
corpus and harness sources are identical to the registered base.

| Gate | Observed result | Interpretation |
| --- | --- | --- |
| F1 | Loop graph is SSA-valid; the second dynamic consume uses already-consumed storage. Fresh-body control allocates/consumes two different resources without violation. | Dominance alone does not establish per-iteration ownership. |
| F2 | Alias graph is SSA-valid; writing its alias accesses consumed storage. Clone control has two allocations and no violation. | Single-assignment names do not establish storage disjointness. |
| F3 | Both Go tests pass with `-race`: in-place update makes both versions observe 2; copying preserves the old value 1. | In-place map_put is not a persistent update while the old map is observable. |
| V1 | 15 Python tests pass, including malformed SSA, edge-specific phi dominance, simultaneous phi assignments, and safe mutually exclusive consumption. | Structural acceptance is not an accept-everything stub. |
| Real Hybrid witnesses | 4/4 match: loop/alias reject with `HYB-E3003`; fresh/copy accept without errors. | The model addresses ownership distinctions that current Tier-0 already enforces. |
| Workspace | All five crates pass separately: hy-check 2693, hy-diag 19, hy-syntax 148, hy-emit 353, hyb 80. Total 3293; zero failed or ignored. | Existing compiler regression suite stays green. |
| Checker corpus | 139/139; managed tripwire not fired. | Baseline verdict agreement only. |
| Differential | 47/47 accept-side build+run, 92/92 reject-side; full agreement. | Baseline pipeline agreement only. |
| Hygiene | Test registration, conflict markers, seven census surfaces, gofmt, and git diff whitespace checks pass. | New tests are invoked by the existing CI jobs. |

Machine-readable experiment outputs are in [model-results.json](model-results.json)
and [hybrid-results.json](hybrid-results.json). The Go observations are also
assertions in `map_put_test.go`; no timing or allocation claim is inferred.

### Local environment adjustments and failed attempts

- The all-at-once workspace build exceeded available disk margin. An early
  cleanup after interrupting its supervising shell also removed an executable
  before the cargo child finished. Neither attempt counted as a passing run.
  Successful replacement validation ran `cargo test -p PACKAGE --locked
  --offline` for **every** workspace package, with `CARGO_INCREMENTAL=0` and
  `RUSTFLAGS="-C strip=symbols"`, freeing completed test executables between
  groups. No tests or compiler sources were altered or skipped.
- The cached Go distribution lacked `runtime/race` sources. The final race
  test used the complete official `go1.26.9.linux-arm64.tar.gz`, SHA-256
  `4a97373d49fcacdcf3694fea368a500b00ee3e963974f3e7514132717632f052`, verified
  against go.dev release metadata. No amd64 package was downloaded.
- Two differential attempts failed Go VCS stamping for corpus 12 and 13
  in sandbox `/tmp` workspaces (45/47 accept-side, 92/92 reject-side).
  `GOFLAGS=-buildvcs=false` did not fix those because hyb overrides GOFLAGS.
  The final complete run used `TMPDIR="$PWD/.local-validation-tmp"` from the
  official checkout and `HYB_RUNTIME_DIR="$PWD/runtime"`; Go then found valid
  repository metadata. No wrapper, fixture, harness, or production change
  was used to bypass those builds. The temporary directory is not committed.
- The initial copy-control fixture used an unavailable `clone` builtin.
  It was corrected to an explicit integer-entry copy and all four real
  Hybrid witnesses were then rerun successfully.

Raw command logs, including unsuccessful attempts, are retained in the local
worker's `tests.log` for gateway review. Hosted CI has not run yet.

### Verdict and limits

The narrow **dominance-alone** P1 claim is falsified by F1 and F2. SSA may
still simplify the checker, but deleting ownership state requires a specified
semantic replacement such as affine capabilities and ownership-aware phi
transfers. This result does not justify removing existing back-edge or alias
checks, and does not establish infeasibility of an ownership-aware SSA IR.

There is no AST-to-SSA frontend, new Tier-0 checker, or SSA Go emitter in this
spike. Consequently P2 (escape-analysis parity), diagnostic-span parity, O8
branch/store-back completeness, and new-checker equivalence on historical
reproducers were **not measured**. Full existing-suite agreement above must
not be read as any of those claims. Original idea credit stays with
`ryder777777`; ledger admission, review, learning publication and any award
remain maintainer decisions. Prepared with AI assistance.
