# Actions artifact digest verification — local evidence

Contribution: issue #4241 / draft PR #4441. Prepared with AI assistance.

## Scope

Validate the optional documented `digest` artifact metadata (`sha256:<64 hex>`)
before downloading and check the received bytes before creating/replacing the
archive. Missing/null metadata keeps legacy behavior. This is the original
contribution reported in #4241, not a second bounty claim.

## Local validation, 13 November 2026

Compiler validation used a clean isolated checkout of published commit
`2675e51ac1ded99c04ce56a13a7093ccb9ae55ad`, Linux x86_64, official Go 1.26.9
and Rust 1.99.0. `rustc --version`: `rustc 1.99.0 (b940084d7 2026-09-28)`;
`cargo --version`: `cargo 1.99.0 (5f94df478 2026-08-27)`;
`go version`: `go version go1.26.9 linux/amd64`.
Downloads were verified against Rust's version manifest and Go's release JSON
before installing locally. Build jobs were limited to two and serialized with
the repository's heavy-job lock. No repository source, tests or lockfiles changed.

| Command (from repository root unless stated) | Result |
| --- | --- |
| `bash tools/check-versions.sh` | CONSISTENT, exit 0 |
| `cd compiler && cargo test --workspace --locked --quiet` | 3293 passed, 0 failed/ignored/measured/filtered; exit 0; 116.488s |
| `cd compiler && cargo build -p hyb --locked --quiet` | exit 0 |
| `python3 tools/corpus_harness.py --bin compiler/target/debug/hyb` | verdict agreement 139/139; tripwire not fired; exit 0 |
| `python3 tools/managed_gate.py compiler/target/debug/hyb gates/managed-baseline.json tests/checker-corpus/02-shared-map-managed.hy tests/checker-corpus/18-pool-shared-struct-managed.hy` | PASS, 2 blocks / baseline 2; exit 0 |
| `python3 tools/differential_loop.py --bin compiler/target/debug/hyb` | FULL AGREEMENT: accept-side build+run 47/47, reject-side checker 92/92; exit 0; 35.815s |
| `git status --short` after the commands | empty |

The earlier published head `8e685a035a1713b4c3b09ef39d8be6ba03b1227a` passed
557 top-level tools tests, 25 fuzz-harness tests and 29 runner tests locally
(611 total, no skips), with registration/conflict/guarantees/claims/census guards.
`2675e51a` differs only in PLAN.md and STATUS.md; the tested code/tests are unchanged.
Original negative reproduction and focused 14-test regression results remain
recorded in #4241. No competitor's source was copied.

## Download provenance

Rust manifest: `https://static.rust-lang.org/dist/channel-rust-1.99.0.toml`.
Rust archives came from its `x86_64-unknown-linux-gnu` entries under the official
`https://static.rust-lang.org/dist/2026-10-01/` directory.
Go release metadata: `https://go.dev/dl/?mode=json&include=all`;
archive: `https://dl.google.com/go/go1.26.9.linux-amd64.tar.gz`.

| Archive | Verified SHA-256 |
| --- | --- |
| rustc-1.99.0-x86_64-unknown-linux-gnu.tar.xz | 77171ba2a0345fdf2abc4fedda55d6de078dae7a68527c28be8c77dcc9604bd5 |
| rust-std-1.99.0-x86_64-unknown-linux-gnu.tar.xz | 3e58dff2d0b72196b5ea4e90536e174d400de88564a52694686b81e091169933 |
| cargo-1.99.0-x86_64-unknown-linux-gnu.tar.xz | d7674918d28093097614cd9728b6ca60db9ea3038f640f0bd1e9a4188c7568ce |
| go1.26.9.linux-amd64.tar.gz | 42d158b4d8f7b61ac0a830567c940a86098fb7aac52e467a5ebec03ef5cc2f8d |

## Limits

These are local results, not a hosted CI pass. The earlier upstream PR workflows
reported `action_required`, with no jobs in the CI run; they were not bypassed
or rerun. No live Actions artifact download was exercised, and not every job in
the full CI workflow (for example the release integration sample) was run here.
Keep the PR draft pending the remaining review/CI gates. No merge, award,
reserved funds, payout setup or received payment is claimed.
