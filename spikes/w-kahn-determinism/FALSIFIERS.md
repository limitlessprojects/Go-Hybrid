# #2475 H1 re-test: falsifiers against the real pipeline

**Proposal author:** `@theheiszt` · **Re-tests:** the spike merged in PR #2481

## Why a re-test

H1 was pre-registered as a property of *checked Hybrid programs*: a program the
classifier admits must give byte-identical output on every schedule. PR #2481
reports 7,000 / 7,000 identical runs, but those runs exercise hand-written Go
transcriptions in `work/kahn_fuzzer.go`. No admitted program went through
`hyb check` → `hyb build` → the emitted binary. The classifier is also a set of
source-text regexes, so its verdict and the program's real channel topology can
disagree.

`falsify.py` runs H1 as registered: classifier verdict, `hyb check` verdict,
then N runs of the built binary under `GOMAXPROCS` 1..8 and
`GODEBUG=asyncpreemptoff=0`.

## Falsifiers

Each program is admitted by `kahn_classifier.py` (traced rule by rule below)
and accepted by hy-check, but has a schedule- or run-dependent output.

| Program | Hole in the classifier | Correct verdict |
|---|---|---|
| `f1-helper-spawn-two-producers.hy` | Producers are counted per function body. Four `task` spawns, one per call to a helper, share one channel. | REJECT · E3501 |
| `f2-bidirectional-chan-producers.hy` | Only `chan<-` / `<-chan` parameters are counted. A plain `chan int` parameter (accepted by hy-check; corpus #62, #63) is never counted. | REJECT · E3501 |
| `f3-main-is-second-producer.hy` | Only `task` arguments are endpoints. `main` sending on the channel it shared is invisible. | REJECT · E3501 |
| `f4-map-iteration-via-param.hy` | E3506 only tracks `x := map[...]` bindings and loops naming that same variable. Iterating through a parameter evades it. Go map order is randomized (SPEC-LANGUAGE T8). | REJECT · E3506 |

Why each program is admitted by `classify_hy_source`:

- **f1:** no `for` block contains a `task`, so rule A never fires. Rule B finds
  one spawn in `spawn_sender` and zero in `main`.
- **f2:** rule B's `zip(args, c_params)` sees `out chan int`, which contains
  neither `chan<-` nor `<-chan`. Nothing is counted.
- **f3:** rule B counts `out` once. The `out <- 2` in `main` is not a `task` argument.
- **f4:** `map_vars = {"scores"}`. The loop is `for k, v in t`, so the regex
  `in scores` never matches.

## Measured result

Run on 2026-11-01 against `main` at `92eba51`: `hyb` built with rustc 1.99.0,
Go 1.26.9 (the pins in `tools/env.sh`), on a 16-thread Xeon E5540, Linux x86_64.
N = 1,000 runs per program, cycling `GOMAXPROCS` 1..8, `GODEBUG=asyncpreemptoff=0`.

| Program | Classifier | hy-check | Distinct outputs / 1,000 | Most frequent outputs | H1 |
|---|---|---|---:|---|---|
| f1 | ADMIT | ACCEPT | **12** | `order=4123` ×737, `1243` ×146, `1423` ×44, `4231` ×26 | **falsified** |
| f2 | ADMIT | ACCEPT | **12** | `order=4123` ×732, `1243` ×132, `1423` ×54, `4231` ×30 | **falsified** |
| f3 | ADMIT | ACCEPT | 1 | `order=21` ×1000 | not observed |
| f4 | ADMIT | ACCEPT | **8** | the 8 rotations of `a..h`, ~125 each | **falsified** |

**H1 is falsified: 3 programs admitted by the classifier and accepted by
hy-check produced 8–12 distinct outputs each.** One such program is enough under
the pre-registration.

f3 is still a classifier hole: two producers share one channel. It didn't
diverge here because `main` reaches its send before the freshly spawned task
runs. Its output depends on the scheduler, but this fuzzer can't reach the
other order. It is recorded as "not observed", not as evidence of determinism.

## What this means for the hypotheses

- **H1: falsified** (table above). This is the pre-registered falsifier: an
  admitted, checker-accepted program with two or more distinct outputs.
- **H3:** f1 is the decisive case. A sound endpoint-uniqueness check must
  follow channel endpoints across calls, which is the interprocedural channel
  tracking the H3 falsifier names. The 14 ms / zero-interprocedural figure
  holds only because the classifier skips that analysis.
- **H2 is unaffected.** It measures false negatives, and these are false positives.

None of this weakens the idea. Kahn's theorem still holds for programs that
really are single-producer, single-consumer. The result is that endpoint
uniqueness can't be judged from syntax. It has to be computed from hy-check's
ownership data, and that is where the analysis cost belongs.

## Run it

`hyb` enforces the toolchain pins at build time and at `hyb build` time
(HYB-E9001), so both steps need the pinned Rust and Go on hand:

```sh
export RUSTUP_TOOLCHAIN=1.99.0 PATH=/path/to/go1.26.9/bin:$PATH
(cd compiler && cargo build -p hyb)
python3 spikes/w-kahn-determinism/falsify.py --bin compiler/target/debug/hyb --runs 1000 --json /tmp/kahn-falsify.json
```

Record every outcome, including rows that don't diverge.
