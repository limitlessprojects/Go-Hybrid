# W-2220 M1 — GUARANTEES as code: executable claims and a demotion gate

**Proposal:** [#2220](https://github.com/limitlessprojects/Go-Hybrid/issues/2220) · **Status:** M1 spike, self-contained, no production files touched.

The repo already landed two halves of the claims-table idea:

- **#710** (`tools/check_guarantees.py`, #2001): `guarantees.json` is the source of the table, and structural checks bind rows to rule ids, fixtures and the rendered doc.
- **#2221** (`tools/check_claims_links.py`): every evidence link and corpus verdict marker is checked.

This spike adds the part neither half has: **a claim is only as strong as the verifier the gate actually executes, and a claim that stops executing demotes itself instead of surviving as prose.**

## What M1 adds

1. **Typed, executable verifiers.** `demotion_policy.json` names a verifier per row from the closed set `corpus_pair | run_gate | runtime_pin | argument`. M1 executes three of them:
   - `corpus_pair` (row 1) — runs `hyb check` on the row's reject/accept fixtures; `accept-managed` fixtures must also fail under `--deny-managed`.
   - `run_gate` (row 8) — runs `tools/managed_gate.py` against the committed baseline, **and** injects a new managed block and requires the gate to reject it.
   - `runtime_pin` (row 16) — proves the negative: `hyb check` accepts a send-on-closed program, the emitted Go is run under `go run -race`, and the pinned panic text must appear.
2. **Demotion state machine.** `executable → prose-only (owner + expiry + debt ticket) → demoted to no claim`. A guaranteed row may not be prose-only; an expired prose-only row renders as **demoted**, never as a live claim. Rows without a policy entry render as `unregistered (M1 out of scope)` — silence is not a claim.
3. **Rendered table with drift detection.** `--render` emits `rendered_claims.md`; the gate fails when the committed render differs.
4. **Seeded-mutation self-test.** `--self-test` mutates the registry/policy five ways (fixture verdict flip, policy drop, guaranteed-as-prose, wrong panic text, expired prose) and fails unless **all five** are caught. A gate that cannot fail is not a gate.

## M1 scope

| Row | Verifier | What the gate executes |
|-----|----------|------------------------|
| 1 | `corpus_pair` | 3 fixtures: two REJECT, one ACCEPT |
| 8 | `run_gate` | 2 baseline managed blocks pass; an injected new block is rejected |
| 16 | `runtime_pin` | checker accepts; emitted Go panics `send on closed channel` under `-race` |
| 9 | `argument` (prose-only demo) | expires 2026-12-31, debt `W2220-DEBT-009`; renders as no-claim, not as a guarantee |

Rows 2–20 outside those four are deliberately `unregistered` in M1: wiring all 20 is M2.

## Run it

```bash
# from the repository root, with `cargo build -p hyb` done and Go on PATH
python3 spikes/w2220-guarantees-as-code/guarantees_gate.py
python3 spikes/w2220-guarantees-as-code/guarantees_gate.py --self-test
python3 spikes/w2220-guarantees-as-code/guarantees_gate.py --render

# optional explicit toolchains
python3 spikes/w2220-guarantees-as-code/guarantees_gate.py \
    --hyb compiler/target/debug/hyb --go go

python3 -m unittest discover -s spikes/w2220-guarantees-as-code -p 'test_*.py' -v
```

Exit codes: `0` all executable rows verified and the render matches; `1` a violation or a missed self-test mutation; `2` CLI misuse.

## Files

| File | Role |
|------|------|
| `guarantees_gate.py` | verifier runner, demotion state machine, renderer, self-test |
| `demotion_policy.json` | per-row status/verifier/owner/expiry/debt |
| `rendered_claims.md` | generated table (drift-checked) |
| `test_guarantees_gate.py` | 8 unit/integration tests |
| `RESULTS.md` | recorded run on the pinned toolchains |

## Falsifier (pre-registered with the proposal)

M1 fails if any of these holds:

1. row 1, 8 or 16 has no executable verifier, or its verifier no longer detects the shape it pins;
2. fewer than **5/5** seeded mutations are caught;
3. an expired prose row still renders as a claim;
4. a guaranteed row can be demoted to prose without failing the gate.

Current status: **3/3 rows executed, 5/5 mutations caught, 8/8 tests green.**

## Honest limits (M2)

- Only rows 1/8/16 are wired; the closed verifier set also names `differential`, which is not implemented here.
- Debt tickets are local JSON entries, not GitHub issues; expiry is evaluated at run time, not scheduled.
- The render is a spike-local table, not `GUARANTEES.md`; CI/Makefile wiring and a migration of the 20 rows are M2 work.
- No prototype is claimed beyond this gate. If the approach cannot express 12 of 20 rows as executable checks without new language features, the wall is the published outcome.
