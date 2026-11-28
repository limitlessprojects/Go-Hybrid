# W-4176b — `hcert-v2`: certificates over the emitted Go, closed under successors

A bounded-model-check certificate for Hybrid's concurrency subset, with the
one property a certificate for an *exhaustive* search must have and #4226's
does not: **you cannot pass it by omitting the bad state.**

```
hcert emit   <emitted.go> -o prog.hcert     # enumerate + certify
hcert verify prog.hcert --go <emitted.go>   # O(V+E) replay, re-derives the model
hcert recheck <emitted.go>                   # full enumeration, for POR equivalence
```

One Go binary, standard library only. No Python is needed to replay a
certificate.

## Why this is not #4226 again

#4226's `verify_hcert_replay` checks that every state **listed** in the
certificate is reachable from `s0`. It never checks that every successor of
every listed state is also listed — so a certificate that omits the
transition into the racy state, and recomputes its own digest, verifies
cleanly. `hcert-v2` adds:

- **successor closure** (`HYB-CERT-E006`): for every state and every
  enabled transition not pruned by POR, the successor state *and its edge*
  must be present;
- **program binding** (`HYB-CERT-E002`): the certificate names the SHA-256
  of the emitted Go it describes;
- **model re-extraction** (`HYB-CERT-E003`): the verifier re-parses the Go
  file and rejects a certificate whose model is not that file's model;
- **POR witness** (`HYB-CERT-E009`): the verifier recomputes the persistent
  set and checks every pruned transition is independent of it;
- **a real model**: extracted from the Go AST, not from regexes over the
  `.hy` text and not from the `// EXPECTED-VERDICT` comment.

`TestF1_OmittedSuccessor` demonstrates the gap directly: the same tampered
graph is accepted by a faithful port of #4226's validator and rejected by
`hcert verify` with `HYB-CERT-E006`.

## Reproduce everything

```sh
cd spikes/w4176-go-oracle
./run.sh
```

`run.sh` builds `hyb`, emits Go + race binaries for every `ACCEPT` corpus
program, runs the Go tests (tamper suite, F1), enumerates and certifies,
replays every certificate, runs each binary 60 times under `-race`
(`GOMAXPROCS ∈ {1,4,8}`), and prints the table in `RESULTS.md`.

Environment: go1.21.0, rustc 1.84.0. `run.sh` reads both versions and
passes them as `HYB_PINNED_RUST` / `HYB_PINNED_GO`, so the compiler's own
pin guard accepts the local toolchain without editing the repo.

## Files

| Path | What |
|---|---|
| `PREREG.md` | the pre-registered contract, schema and falsifiers F0–F7 |
| `RESULTS.md` | the measured table (all numbers from `./run.sh`) |
| `internal/model` | Go-AST → bounded concurrency model |
| `internal/lts` | interleaving enumeration, vector clocks, persistent-set POR |
| `internal/cert` | `hcert-v2` emit, replay verifier, tamper suite |
| `cmd/hcert` | `emit` / `verify` / `recheck` / `corpus` |
| `cmd/oracle` | the `-race` differential harness |
| `tools/summary.py` | renders the numbers from the run's JSON |

## Scope

The certificate covers programs that `hyb` emits Go for and that fit the
pre-registered bound (≤ 4 spawned tasks, Σ channel capacity ≤ 8, ≤ 8
synchronising loop iterations, no `select`/`defer`/`unsafe`/foreign calls).
Anything else is reported `OUTSIDE_BOUND` with a reason code — never
silently admitted.
