# Contributing to Hybrid

Short version: we keep it simple, we test everything, and we credit everyone.
Thank you for helping.

## Where to start

- [`good first issue`](https://github.com/limitlessprojects/Go-Hybrid/labels/good%20first%20issue)
  issues are tagged and self-contained.
- High-leverage right now: docs, adversarial corpus tests (`.hy` programs
  with an EXPECTED-VERDICT line), diagnostic wording, example programs.
- Soundness bugs have their own track: see [SECURITY.md](SECURITY.md) and
  the [BOUNTY.md](BOUNTY.md) pool.
- **Crazy ideas have a funded home too**: the Wildcard Track pays for
  revolutionary, contrarian, high-risk proposals a normal feature bounty
  would reject — **no prototype required**, breaking changes, rewrites,
  and "impossible" ideas explicitly allowed, and failures are published as
  learnings (and paid). Submit via the
  [`wildcard` template](https://github.com/limitlessprojects/Go-Hybrid/issues/new?template=wildcard.yml);
  the ledger, the review calendar, and the failure wall live in
  [MOONSHOT.md](MOONSHOT.md).

## Setup

The pins are the contract: Go 1.26.9 and Rust 1.99.0.

```bash
git clone https://github.com/limitlessprojects/Go-Hybrid.git && cd Go-Hybrid
make test-quick   # compiler tests + checker corpus + differential loop
make examples     # the ten-program acceptance suite
```

Repro culture: every behavior change ships with a test that fails without
it. Tests are never weakened to pass. See [AGENTS.md](AGENTS.md) for how
the repo runs.

## Pull requests

1. Small and focused beats large and sweeping.
2. `cargo test --workspace`, the corpus, and the differential loop must be
   green locally, then green on our GitHub Actions before final review.
3. Grammar or checker changes need a SPEC-LANGUAGE note and a corpus or
   test pin. Verdict changes go through a logged amendment.
4. Every PR gets a written reply, merged or not. No silent closes.
5. The queue self-drains: a PR older than 5 days without a protected label
   (`triage:pareto`, `bot:land-candidate`, `triage:wildcard-hold`, `triage:keep`) is closed as
   not-planned by the [queue sweeper](AUTOMATION.md) — with the written
   reply from rule 4 explaining the reopen path. Rebased reopens re-enter
   triage as fresh arrivals, and the daily board promotes land-candidates
   automatically.

## Recognition

We recognize contributions publicly and concretely. The roles, how to earn
them, and the All-Contributors bot command are in
[CONTRIBUTOR_RECOGNITION.md](CONTRIBUTOR_RECOGNITION.md). Winners of the
bounty cycle are announced in release notes and the Hall of Fame.

## Community

We are **working on a Discord**; it will be linked here the day it opens.
Until then, [Discussions](https://github.com/limitlessprojects/Go-Hybrid/discussions)
is the town hall and every thread gets an answer.

Before starting, star the repository if you plan to stick around. It helps
us grow and shows you are committed to contributing!
