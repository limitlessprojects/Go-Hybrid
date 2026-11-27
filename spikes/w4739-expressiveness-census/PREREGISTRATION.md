# Expressiveness census pilot, issue #4739

This is an implementation/replication contribution to chyalls's proposal,
not a new wildcard proposal or a claim to its author's prize.

Registered before port experiments: 2026-11-28 (Europe/Istanbul).
Compiler baseline: 82037c3da4f3b888eef2e21ee1cb5e347e74a0c4.
Toolchains: Go 1.26.9, Rust 1.99.0; no toolchain-drift override.
Upstream: golang.org/x/sync v0.18.0, commit
1966f539bbd7664efd5bb7462ae94d9db67f4502, BSD-3-Clause.

## Frozen pilot

1. errgroup: production implementation and its unchanged upstream tests,
   including the parallel-search example, errors, cancellation, limits.
2. singleflight: production implementation and its unchanged upstream tests,
   including duplicate suppression, errors, Forget, panic and Goexit.

These production implementations exceed the proposal's stated <150-line
size target. This is reported, not hidden by truncating the source or
deleting its panic/cancellation path. The source-size mismatch and any
language-surface blocker must be reviewed separately from Tier-0 costs.

Freeze naive translations before acceptance edits. Record their hashes.
Every acceptance edit needs an observed diagnostic and a reversal check;
semantic changes must be disclosed, not called syntax normalization.

Primary metric and thresholds remain those of #4739: real added/deleted
lines against the original source (modified lines are represented once as
a deletion plus an addition, not counted a third time). Also report the
naive-Hybrid to accepted-Hybrid diff to distinguish syntax from redesign.
Never infer a zero managed/escape count from missing evidence.

CLEAN requires no post-reject edits and no escapes; REDESIGN requires an
accepted, behavior-tested port without managed/Go-API escapes; ESCAPED
requires at least one such escape, explicitly inventoried. An accepted
checker verdict alone is not a passing behavior test. Infrastructure
failure is not a checker rejection. Missing behavior evidence is not PASS.
UNPORTABLE requires independent countersignature; until then use
REVIEW_REQUIRED or SURFACE_BLOCKED, and exclude it from aggregate verdicts.

The fixed nine-program list and decision thresholds in #4739 are not
replaced by easier examples. The remaining seven programs and the
independent second-porter review are outside this pilot. With fewer than
the required reviewed ports, the census verdict is INCONCLUSIVE.

The untranslated-Go sweep will use a fixed manifest of real Go source
files from this same upstream module. Parse/lexical errors are reported
separately and are not evidence of ownership/sendability rejection.

## Falsifiers

- An unchanged file must have zero rewrite tax.
- A missing input, invalid checker JSON, timeout or checker exit >1 must
  prevent a success verdict.
- Changing a checked fixture must invalidate frozen hashes.
- A failed upstream test prevents behavioral-equivalence claims.
- A Go helper retained to preserve unsupported semantics counts as an
  escape; it must not be credited as checked Hybrid code.

No self-awarded M1/M2 acceptance, payment, or ledger edits.
