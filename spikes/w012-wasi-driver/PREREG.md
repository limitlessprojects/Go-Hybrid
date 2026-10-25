# W-012 / issue #268 — pre-registration: the stock-checker WASI boundary

**Status:** pre-registered 2026-10-23, before the reported measurement run.
Committed *with* the harness so the commit order proves the order.
Entries follow `spikes/<name>/RESULTS.md` convention: hypothesis → gates →
method → raw results, with the results added to RESULTS.md afterwards.

## Why this spike exists

W-012 (issue #268, slot 12 of the 2026-11-01 window) proposes running the
checker in-process so an editor pays under a millisecond instead of 15–40 ms
of process spawn, and it names two engines: a self-hosted Go rewrite, or
`hy-check.wasm` inside `wazero`. The expensive endcap is a purpose-built
engine ABI (`hy-wasm` cdylib + a hand-written export surface: a second
contract to version, fuzz and keep in sync). The cheap endcap is the binary
we already ship, compiled to `wasm32-wasip1` and run as a WASI command — no
new crate, no new ABI, no second contract.

This spike measures the cheap endcap honestly, on the current corpus, and
against the native subprocess on the same machine. It answers one question:
**is the WASI boundary itself where the win lives?**

## Hypotheses and pre-registered gates

Thresholds are taken from the proposal's own text (issue #268: "15–40 ms in
OS process creation ... an in-process `analysis.Analyzer` runs directly
within `gopls` in under 1 ms"), not from any number this spike had seen.

| id | hypothesis | gate (decided before the reported run) |
|----|------------|----------------------------------------|
| **H1** | The WASI boundary preserves checker semantics exactly | **Accepted** only at 126/126 corpus verdict agreement for *both* engines and 126/126 full-envelope identity (verdict + managed flag + every diagnostic code and position). **Falsified** by any mismatch — the mismatch is then named, not averaged. |
| **H2** | In-process checking is sub-millisecond and ≥2× faster than the subprocess at p50 | **Accepted** if in-process p50 ≤ 1 ms *and* ≤ 0.5× native p50. **Falsified** if in-process p50 ≥ native p50. Between those: *inconclusive*, reported as measured. |
| **H3** | The WASI boundary is cheaper than a process boundary | **Accepted** if the invocation floor (same path pointed at a missing input: spawn vs instantiate, no checking) has in-process p50 ≤ 0.5× native p50. **Falsified** if in-process floor ≥ native floor — then the win, if any, is not the boundary. |
| **H4** | Cold start is negligible against one keystroke | **Accepted** if one-time module compile ≤ 100 ms. **Falsified** if > 500 ms: the first diagnostics an editor shows pay it, so "instant" needs a warm daemon or a cached compile. |

No gate is renegotiated after seeing a number. A failed gate is a published
learning, not a reason to re-run until green (BOUNTY.md Track 3).

## Method (fixed before the run)

- **Build:** one commit of this repository; `hyb` native (release) and
  `hyb.wasm` (`wasm32-wasip1`, release) from that same tree, Rust 1.99.0,
  `cargo build --release -p hyb [--target wasm32-wasip1]`. No crate added,
  no source change to the compiler.
- **Parity (H1):** every `tests/checker-corpus/*.hy`, both engines, one
  invocation each, full envelope identity compared.
- **Latency (H2, H3):** a 25-file workspace copied out of the corpus
  (deterministic stride, never the repository itself); 100 sequential edits
  (a trailing comment line — no existing line moves, so a verdict change can
  only come from the checker); one check after each edit; **3 repetitions
  per engine**, engine order alternating each repetition so a warming host
  cannot systematically favour one path.
- **Floor (H3):** 30 invocations per engine against a path that does not
  exist, so the checker does no work and only the boundary is measured.
- **Statistics:** nearest-rank percentiles over the pooled samples; every
  reported number is a real sample, nothing interpolated. Teardown of a WASI
  instance is outside the timed window (documented in the harness).
- **Honest scope:** one host, one core count, files of corpus size, warm page
  cache, no editor. A ratio measured here is a lower bound on the *boundary*
  difference, not an IDE claim.

## Disclosure: what was seen before this file was committed

To prove the harness worked, the following were run before this
pre-registration: (a) single-file smoke invocations on both engines, (b) one
full parity pass (126/126 on both), (c) one 3×100 benchmark whose pooled p50
was 1.486 ms native and 1.510 ms in-process on the 4-CPU box this ran on.

They are disclosed here because they were seen. The gates above do not
depend on them: H2's 1 ms and 0.5× come from the issue's own "under 1 ms"
versus "15–40 ms", and H3's floor comparison is a structural question, not a
number. The reported run is a fresh run with the harness in its final form.

## Amendment 1 (2026-10-25, after the rebase, before the re-run)

This spike's original run was on `main` @ `7d63d5f`. The repo then ran a
queue reset and the PR was closed as not planned; the branch has been
rebased onto `main` @ `1f0ff97` (72 commits later, no conflicts, the
spike's own diff — `spikes/w012-wasi-driver/` only — is unchanged) and
re-run to check that the verdicts still hold.

**What changed and what did not.** The corpus grew from 126 to 127 programs
while this PR was closed (`1f0ff97`-era `main` registers a 127th fixture),
so H1's denominator moves with it. H1's threshold is *zero mismatches over
the whole corpus* — verdict agreement **and** full-envelope identity for
both engines — and that threshold is unchanged. The gate text below reads
"126/126" because that was the corpus at pre-registration time; for this
re-run it is **127/127**, the same gate on the same population rule.

Nothing else is renegotiated: H2's 1 ms / 0.5×, H3's 0.5× floor ratio and
H4's 100 ms / 500 ms boundaries stay exactly as registered, and the numbers
already published in RESULTS.md are left in place as the first run. The
re-run is reported as its own run, on its own host line, with its own raw
artifact — not merged into the first run's tables.
