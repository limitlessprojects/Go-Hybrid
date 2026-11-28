# W-4926 spike: callback-boundary checker for finite-state stream kernels

Issue: [#4926](https://github.com/limitlessprojects/Go-Hybrid/issues/4926)
(Wildcard Track) - "Make callback boundaries unobservable for
finite-state stream kernels."

## The contract

A streaming kernel opts in to a promise: callers may divide the stream
into buffers however they choose - `step(q, x)` followed by
`step(q1, y)` must agree with `step(q, x + y)` on the concatenated
emissions, in order, and on the residual state, exactly. Empty callbacks
preserve state and produce no output. A final flush is a separate
operation and is not part of the promise.

## What this spike does

It builds the review spike the issue proposes, as a standalone
library-level checker (no change to the Hybrid language):

- A kernel inside the fragment is declared as a finite transducer:
  finite state set, finite input alphabet, at most one emission per
  sample, and an explicit *boundary transition* - what a callback
  boundary does to the state.
- `certify()` explores reachable pairs of states for an unbroken
  execution versus one admitting arbitrary boundaries. If no reachable
  pair can diverge, the kernel is **CERTIFIED** inside the fragment.
  If a pair diverges, the verdict is **REJECTED** with a short concrete
  witness (input + cut pattern + both traces) the kernel author can
  replay. A kernel outside the fragment (unbounded state, flush output,
  more than one emission per sample) is **declined**: no certificate,
  no rejection.
- First kernel: the issue's stride-3 audio decimator with explicit
  phase. Carrying the phase certifies; resetting it at every callback
  is rejected with a two-sample witness.
- The empirical leg reproduces the issue's independent Python probe
  number for number (see RESULTS.md).

## Run

```bash
python3 callback_boundary_checker.py          # verdicts + probe numbers
python3 -m unittest discover -s tests -v      # 11 tests
```

Python 3 standard library only.

## What a certificate does not claim

A certificate covers only the declared fragment: the transducer as
declared, over its finite alphabet, with no flush. Extracting a faithful
transducer from arbitrary kernel code, dynamic allocation,
nondeterminism, lookahead filters, and arbitrary DSP graphs are outside
this spike, as the issue's cost-honesty section scopes them. A rejection
of the carry kernel, or a certificate for the reset kernel, would
falsify the spike.
