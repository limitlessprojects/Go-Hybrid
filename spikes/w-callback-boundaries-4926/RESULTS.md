# Spike Results: W-4926 (Issue #4926)

## Summary

Standalone checker spike for the Wildcard Track proposal "Make callback
boundaries unobservable for finite-state stream kernels". The checker
models callback boundaries as separate transitions on a finite
transducer and explores reachable pairs of states for an unbroken
execution versus one admitting arbitrary boundaries, as the issue's
review-spike paragraph specifies. It is a library-level checker: no
Hybrid language or compiler change.

## Pre-registered expectations (from the issue, before running)

- Carry-phase decimator (stride 3): CERTIFIED.
- Reset-phase decimator: REJECTED with a short witness of the
  [0] | [1] class (one batch emits one sample, restarting emits both).
- Independent probe reproduction, exact: 43,690 cases over binary
  inputs of lengths 1-8 under every nonempty-block partition; carry
  misses 0; reset misses 41,084 output traces and 41,916 index-tagged
  traces; empty callbacks preserve all 3 phases under carry; a
  48,000-sample ramp emits 16,000 in batch and 16,000 / 16,252 /
  16,125 / 16,031 when restarted at fixed widths 96 / 127 / 128 / 1,024.
- Kernels outside the fragment (unbounded state, flush output) receive
  no certificate.

## Raw results (run 2026-11-29, Python 3, stdlib only)

```
decimator-carry: CERTIFIED (pairs explored: 3)
decimator-reset: REJECTED (pairs explored: 3)
  witness (emission): input [0, 0] as blocks [[0], [0]]
  unbroken emissions [0] (final state 2); chunked emissions [0, 0] (final state 1)
global-counter: OUTSIDE_FRAGMENT (state set is not finite)
decimator-with-flush: OUTSIDE_FRAGMENT (kernel emits at a final flush (flush is a separate operation))
probe: {'cases': 43690, 'carry_misses': 0, 'reset_misses': 41084, 'tagged_changes': 41916}
empty callbacks: {'carry_preserves': True, 'reset_violating_phases': [1, 2]}
ramp: {'batch': 16000, 'restarts': {96: 16000, 127: 16252, 128: 16125, 1024: 16031}}
ALL CHECKS PASSED
```

Unit tests: 11 ran, all passed (`python3 -m unittest discover -s tests`),
including a faithfulness test that folding the transducer over every
partition of every binary input up to length 6 reproduces the issue's
block-level probe semantics for both modes, and a replay test that the
returned witness actually diverges at block level.

## Notes on the witness

The checker's shortest witness for the reset kernel is the input
[0, 0] cut as [[0], [0]]; the issue's hand-picked example is [0] | [1].
Both are minimal (two samples, one cut, second emission appears only in
the chunked run); a dedicated unit test pins the issue's exact example
against the block-level oracle. The checker also records residual-state
divergence (the reset kernel reaches pair (phase 1, phase 0) after one
sample plus a boundary); the emission witness is reported first because
it is the more concrete diagnostic.

## Falsification conditions

- A rejection of the carry-phase kernel, or a certificate for the
  reset-phase kernel, falsifies the checker.
- Any probe number differing from the issue's falsifies the claimed
  reproduction.

## Limits

The certificate covers only the declared fragment: a finite transducer
over a finite alphabet, at most one emission per sample, no flush
output. Extracting a faithful transducer from arbitrary kernel code is
not attempted; dynamic allocation, nondeterminism, filters with
lookahead, and arbitrary DSP graphs remain outside the first milestone,
matching the issue's cost-honesty scoping. Within the fragment, the
pair exploration is exhaustive over reachable state pairs, so a
certificate is a decision for all inputs and all cut patterns over the
declared alphabet - not a bounded sample.
