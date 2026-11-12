# W-3269 Spike Results — Affine Effect Permits and Explicit Uncertain Outcomes

**Proposal:** [#3269](https://github.com/limitlessprojects/Go-Hybrid/issues/3269) · **Verifier:** `python3 spikes/w-affine-effect-permits-3269/effect_permit_verifier.py --check` · **Tests:** `python3 -m unittest discover -s spikes/w-affine-effect-permits-3269/tests -p 'test_w3269_spike.py'`

This is the M1 artifact: a finite model and its pre-registered gates, as the proposal asks for ("before syntax work, agree a small finite model"). No compiler change is claimed and none is made. The honest result is stated under "What this does not establish".

## 1. Pre-registered gates

The five gates are the proposal's own numbered list, in its order, with its own wording as the acceptance condition.

| ID | Gate (from #3269) | Measured | Verdict |
| :--- | :--- | :--- | :--- |
| **1** | Non-vacuous positive controls: known outcome completes; a local preflight failure returns an unused permit that subsequently succeeds; one unresolved operation does not prevent an independent permit from succeeding | All three constructible in the model: `Known` terminates with `emissions=1`; `NotStarted` leaves `holder=p1` and a second attempt reaches `Known` with `emissions=1`; `op1` in `Uncertain` does not block `op2` on `p2` | **PASS** |
| **2** | Lost-ack trace: adapter records one emission, receiver applies it, reply disappears. A retry loop must not obtain a second initiation right; an observation returning "not found" must not manufacture one | Emission frozen at 1; `try_retry_with_pending` has **no transition** (returns `[]`); an "empty lookup" observation leaves the token in place with `holder` still `None` and no new right | **PASS** |
| **3** | Conservation across control flow: duplicate captures/transfers, stale generations, early return, cancellation must not duplicate a permit or erase a pending obligation | Second transfer from the original owner is impossible (owner check); transfer to `custodian` keeps `p1` in `pending`; a stale owner's `observe` returns no states | **PASS** |
| **4** | Counterfactual controls: return a permit on post-initiation timeout, forge a terminal receipt, lower a pending value through generic error propagation — each must be exposed | All three violate an invariant and are caught: `I1b` (right-and-obligation coexist), `I2 duplicated` (pending+known), `I2 erased` (permit in none of the three states) | **FIRED** |
| **5** | Compiler feasibility (model half): compare against a plain `(T, error)` retry control | Plain retry issues 3 emissions over 3 retries; the permit model stays at 1 across the same 3 retries. The two are distinguishable in the model | **PASS (model half)** |

Exploration: **81 reachable states**, bound 20000 never reached, so the enumeration is **exhaustive for this model**.

## 2. The model

Two permits (`p1`, `p2`), two tasks (`t1`, `t2`), two protected operations (`op1`, `op2`). State is `(holder, pending, known, emissions)`; every transition is the contract exactly as written:

```text
attempt(move permit, payload)
    -> NotStarted(permit, cause) | Known(receipt) | Uncertain(pending)
observe(move pending)
    -> Known(receipt) | Uncertain(pending)
```

`attempt` has exactly three successors. `NotStarted` returns the state unchanged (no emission). `Known` consumes the permit, records a receipt, increments the emission counter. `Uncertain` consumes the permit, installs a pending token, increments the emission counter — and **never** puts the permit back.

That last sentence is the whole proposal, and it is three lines of code here.

## 3. Invariants

| ID | Statement | Holds |
| :--- | :--- | :--- |
| **I1** | No operation is initiated more than once per permit; total emissions per operation never exceed the permit count | yes, on all 81 states |
| **I1b** | A permit never simultaneously holds an initiation right and a pending obligation or receipt | yes |
| **I2** | Every permit is in exactly one of {held, pending, known} — never erased, never in two at once | yes |

## 4. What this does not establish

Being precise about the boundary matters more than the PASS.

- **Not a compiler proof.** 81 states is a finite protocol model. It says nothing about whether Hybrid's checker can *enforce* the discipline across closures, task transfers, early returns and its Go/Rust boundary. Gate 5 is only half-satisfied: the plain-retry control exists in the model, the emitted-Go/Rust trace comparison does not.
- **No syntax, no admission rule, no lowering check.** The proposal explicitly sequences syntax work after the model, and this stops at the model.
- **The trusted base is large and unmodelled beat.** The issuer, the adapter, and the adapter's own transport retries are all assumed correct here. The proposal says an adapter that retries internally can still emit twice; this model cannot see that, because it models the permit, not the wire.
- **Exactly-once is not claimed**, and the proposal does not claim it either. Two distinct permits for one business action remain possible; an unresolved intent can stay unresolved forever; no durability, dedup, rollback or bounded completion.
- **The bound was never reached, but the bound is still a choice.** If a future state space grew past it, the verifier returns UNKNOWN rather than PASS — that is deliberate (see `explore`), but it means the exhaustive claim is scoped to *this* state shape, not to any extension.

## 5. Where I would go next, in order

1. **Gate 5's missing half.** Write three tiny programs — plain `(T, error)` retry, hand-written state machine, and the proposed permit shape — and compare emitted Go traces against the model's emission counts. This is the falsifier the proposal names: if an ordinary library already gives the same enforceable contract, the proposal should be narrowed or declined, and that is a *good* result.
2. **Cancellation.** The model treats cancellation as "no transition", which is correct but thin: a real cancellation lands between initiation and acknowledgement, which is exactly the `Uncertain` branch. Worth an explicit state for it rather than silence.
3. **Early return.** Currently modelled as "the obligation stays with the frame", which only works if the frame outlives the obligation. Returning a pending token from a function is the interesting case and is not covered.

## 6. Falsifiers I looked for and did not find

- A reachable state that double-initiates: none in 81 states.
- A reachable state that erases an obligation: none.
- A gate that passes vacuously: gate 1's three controls are constructed, not asserted; gate 5 compares two different numbers (3 vs 1) rather than asserting a constant.

The falsifier I *would* expect to fire with more work is gate 5's full form: an ordinary Go library expressing the same contract more simply. That has not been tested and should be tried before any syntax proposal.

## Reproduce

```sh
python3 spikes/w-affine-effect-permits-3269/effect_permit_verifier.py --check
python3 -m unittest discover -s spikes/w-affine-effect-permits-3269/tests -p 'test_w3269_spike.py'
```

Expected: `PASS`, 81 reachable states, 5 gates ok; 7 tests OK.

## 7. Gate 5, trace half — run after the first draft of this file

The proposal asks gate 5 to "compare tiny admitted programs and their emitted Go/Rust traces against it. Include a plain `(T, error)` retry control and an explicit hand-written state-machine control."

Three programs, each attempting one protected operation, losing the ack, then retrying twice. The counter is how many times the protected operation actually ran.

| Shape | Emissions | Note |
| :--- | :--- | :--- |
| plain `(T, error)` retry | **3** | re-issues on every error, the status quo |
| hand-written state machine (correct) | **1** | refuses to re-initiate: permit spent |
| proposed permit discipline | **1** | same count, same refusal |

That alone reads as a **narrow signal**: a hand-written state machine already achieves the discipline, so language support adds nothing *expressiveness-wise*. I wrote that down before running the next variant, because it is the honest first reading.

The second variant is what changes the conclusion:

```go
func tryOnceBuggy(p *permit, failAck bool) (string, error) {
    if p.used { return "", errors.New("no right") }
    p.used = true
    _, err := protect(failAck)
    if err != nil {
        p.used = false   // THE BUG: "it failed, so give the right back"
        return "", err
    }
    return "receipt", nil
}
```

```
correct=1 buggy=3
CONFIRMED: the discipline is expressible by hand; only enforcement
           distinguishes correct from buggy.
```

Returning the permit on a post-initiation failure is not a contrived mistake — it is the *natural* thing to write, because "the call failed" reads as "nothing happened". Nothing in Go, and nothing in Hybrid's current checker, prevents it. Both versions compile, both look reasonable, one emits three times.

### What this changes about the verdict

Gate 5's falsifier was "an ordinary library offers the same enforceable contract more simply". The trace comparison says something sharper:

- **Expressiveness**: an ordinary hand-written state machine is enough. The proposal is not needed to *say* the contract.
- **Enforcement**: the hand-written version is correct only by author discipline. The plausible wrong version is one line away and silently re-emits.

So the proposal's actual product is not a new way to express the protocol — it is a checker obligation that makes the wrong version a compile error. That reframes what M2 should try to demonstrate, and it narrows the claim: not "Hybrid can express affine effect permits" (a library can), but "Hybrid can *reject* the permit-restoring mistake".

Recorded as **PASS-WITH-NARROW-SIGNAL** in the verifier, and the narrowing is stated in the PR rather than left for a reviewer to find.

