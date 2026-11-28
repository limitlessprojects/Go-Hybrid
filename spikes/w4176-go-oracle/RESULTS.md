# W-4176b Results — Exhaustiveness-Closed Certificates over the Emitted Go (#4176)

All numbers below are produced by one command, `./run.sh`, on this machine
(go1.21.0, rustc 1.84.0, darwin/arm64). Nothing is estimated.

## 0. What was measured

For every corpus program whose `EXPECTED-VERDICT` is `ACCEPT` or
`ACCEPT-MANAGED`, `hyb build --race` emits Go and a race-enabled binary.
The certificate tool then:

1. re-extracts a bounded concurrency model **from the emitted Go AST**,
2. enumerates the reachable interleaving graph (buffered channels, mutexes,
   vector clocks) and detects conflicting unsynchronised accesses,
3. emits an `hcert-v2` certificate (full graph for `CERTIFIED`, witness
   path for `RACE_DETECTED`),
4. replays it with an independent verifier that re-extracts the model from
   the Go file and checks **successor closure**, and
5. runs the same Go binary 60 times under `-race`
   (20 runs × `GOMAXPROCS ∈ {1,4,8}`).

## 1. Pre-registered falsifiers

| ID | Threshold | Measured | Verdict |
|---|---|---|---|
| **F0** admission is a real predicate | `>= 1` OUTSIDE_BOUND, including one program #4226 called admitted | Tier B: **2** OUTSIDE_BOUND — `16-move-send-clean` (Σchan=16), `18-pool-shared-struct-managed` (Σchan=10). Both are `ACCEPT` programs #4226 reported as certified. | **PASS** |
| **F1** omission is detectable | delete one successor edge → #4226 accepts, `hcert verify` rejects `HYB-CERT-E006` | `TestF1_OmittedSuccessor`: deleted edge `2->4 (read t0.m)`; the #4226 validator (faithful port) accepts the incomplete graph; `hcert verify` raises `HYB-CERT-E006`. | **PASS** |
| **F2** runtime differential | 100% agreement on the admitted set | **45/45** programs: 44 `CERTIFIED` with **0** race reports over 60 runs each (2,640 runs), 1 `RACE_DETECTED` (`02`) with **60/60** runs reporting a race. | **PASS** |
| **F3** POR soundness | full and POR enumeration agree on 100% of admitted programs; report reduction | **45/45** agree. 10,479 full states → 1,172 certified states (**11.2%**). | **PASS** |
| **F4** precision vs corpus | 100% agreement with `EXPECTED-VERDICT` on the admitted set | **45/45** (`ACCEPT` → `CERTIFIED`; `ACCEPT-MANAGED` → `CERTIFIED` or a race witness). `02` is the only `RACE_DETECTED`, matching its pre-registered `RUNTIME-HAZARD`. | **PASS** |
| **F5** tamper suite | `>= 6` attacks rejected with distinct codes | **8/8**: drop-state (`E007`), drop-edge (`E006`), add-illegal-edge (`E005`), flip-access-mode (`E005`), transplant-program (`E002`), bad-digest (`E008`), mutate-model (`E003`), POR-pruned-set (`E009`). | **PASS** |
| **F6** cost | emit `<= 150 ms`, replay `<= 15 ms` | emit max **73.2 ms** (44/44 under 150 ms; total 119.4 ms). Replay max **16.9 ms** on `18`; **43/44 under 15 ms**, median **0.085 ms**, total 33.4 ms. | **PARTIAL** — one certificate 1.9 ms over |
| **F7** reproducibility | one command | `./run.sh` builds, emits, certifies, replays, runs the oracle and prints this table. | **PASS** |

## 2. Coverage (measured, not clamped)

| Set | Count |
|---|---|
| corpus `.hy` programs | 139 |
| `REJECT` programs (no Go emitted — out of scope) | 92 |
| `ACCEPT` / `ACCEPT-MANAGED` programs | 47 |
| emitted and built with `-race` | **45** |
| build failures (`jsonbinding` import needs a go1.26 module) | 2 (`12-a7-borrow-by-default`, `13-a7-own-transfer-at-return`) |
| **pre-registered Tier B** (Σchan ≤ 8): certified / race / outside | **42 / 1 / 2** |
| **extended bound** (Σchan ≤ 16, reported separately): certified / race / outside | **44 / 1 / 0** |
| certificates that replay clean | **45/45** |

The extended bound is reported separately because the pre-registered Tier-B
budget is Σchan ≤ 8; raising it after seeing two programs miss it would not
be a pre-registered pass.

## 3. The largest certificates

| Program | POR states | Full states | Emit | Replay |
|---|---|---|---|---|
| `18-pool-shared-struct-managed` | 496 | 6,808 | 73.2 ms | 16.9 ms |
| `02-shared-map-managed` | 6 (witness) | 704,283 | 6.9 s (full check) | 0.26 ms |

`02` is the only program whose *full* (unreduced) enumeration exceeds the
pre-registered 10^5-state budget; the certified artifact is the POR-reduced
graph, and the full run exists only to check POR equivalence (F3).

## 4. Difference from #4226, one line each

| Dimension | #4226 (`spikes/w4176-schedule-enumeration`) | this spike (`spikes/w4176-go-oracle`) |
|---|---|---|
| Model source | regex over the `.hy` text | `go/ast` over the emitted Go that `go build -race` compiles |
| Verdict | `exp_verdict == "REJECT"` — read from the `// EXPECTED-VERDICT` comment | reachable-state search with vector clocks |
| State graph | arithmetic on the loop index (`sid >> i`, `sid % n_tasks`) | real interleaving of per-task step sequences |
| Counterexample | hard-coded f-string | replayable path + the two conflicting accesses with their vector clocks |
| Admission | `min(4, …)`, `min(2, …)` applied *before* `<= 4`, `<= 2` → cannot fail | measured, can fail (2 programs outside) |
| Certificate binding | digest over the certificate's own JSON | digest over `program_sha256` + re-extracted model + states + edges |
| Exhaustiveness | not checked (only "listed states are reachable") | `HYB-CERT-E006` successor closure |
| POR soundness | asserted, not checked | persistent set recomputed by the verifier; pruned transitions carry independence witnesses (`E009`) |
| Runtime evidence | none | 45 programs × 60 `-race` runs; 45/45 agreement |
| Toolchain | Python 3 + a Go test | one Go binary, stdlib only, no Python needed to replay |
| Coverage claim | 139/139 (tautological) | 45/45 emitted ACCEPT programs, 2 honestly outside the pre-registered bound |

## 5. Limitations (stated, not hidden)

- The model is a **conservative over-approximation** of Go: a mutable
  parameter is treated as a shared location even when it is a fresh copy.
  That can produce a false `RACE_DETECTED` (precision loss) but not a false
  `CERTIFIED` (soundness). F4 shows the loss is zero on this corpus.
- `-race` is **one-sided**: it can falsify a `CERTIFIED` verdict but cannot
  confirm one. F2's 2,640 clean runs are evidence, not proof.
- Programs whose Go needs a module dependency outside the pinned toolchain
  (2 of 47) are not built; they are listed, not silently dropped.
- Loops with synchronisation inside are unrolled to at most 8 iterations;
  loops without synchronisation are summarised to one iteration. Both are
  pre-registered in `PREREG.md`.
- Rendezvous (capacity-0) channels disable the POR reduction; none of the
  admitted corpus programs uses one.
