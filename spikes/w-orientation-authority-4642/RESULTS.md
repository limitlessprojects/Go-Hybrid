# W-4642 spike results — binding an orientation decision to the vertices it may change

- **Issue:** #4642 — *[wildcard] Bind an orientation decision to the vertices it is allowed to change*
- **Proposal credit:** `wd1314zn35-hue` (issue author)
- **Spike scope:** `spikes/w-orientation-authority-4642/` only. No compiler crate, no CI config, no dependency added.

This is the M2 learning for the proposal, not a claim that the feature should ship. The
proposal already shipped a runtime model; what was missing is (a) an independent
reproduction, (b) the half it explicitly did *not* model — static enforcement across a
helper boundary — and (c) an honest statement of where the rule stops working. All three
are below, and every number is produced by `measure.py` in this directory.

## 1. Pre-registered hypotheses and falsifiers

Fixed before `measure.py` was run; the falsifier column is what would sink each one.

| ID | Hypothesis | Falsifier |
|----|-----------|-----------|
| **H1** (predicate) | A conservative binary64 filter over exactly representable differences, falling back to exact integer arithmetic on the original coordinates, agrees with the integer shoelace oracle on every case in the pre-registered families. | Any disagreement, or a filter answer on a case whose products overlap. |
| **H2** (runtime authority) | A witness carrying (mesh identity, epoch, ordered triple, coordinate digest) turns every misbinding into a rejection, including a coordinate write that never bumps the revision. | Any control that commits. |
| **H3** (static rule) | The rule is expressible as an indexed type checked intraprocedurally, and rejects all three misbindings *through a helper* without whole-program alias analysis. | An accepted misbinding, or a rule that only works when every call site is inlined. |
| **H4** (cost) | Witness ceremony is O(1) lines per site and the filter costs no more than ~2x the naive comparison. | More than ~2 lines of ceremony per write, or > 2x predicate cost. |

## 2. H1 — the predicate

`measure.py`, two independent random families plus the adversarial one.

| Family | Cases | Naive float64 wrong | Exact fallbacks | Filtered wrong |
|---|---:|---:|---:|---:|
| random, \|x\| ≤ 2^26 | 10,000 | 0 | 0 | 0 |
| random, \|x\| ≤ 2^49 | 10,000 | 0 | 0 | 0 |
| unit determinant, n = 2^27…2^49, both orders | 46 | **46** | **46** | 0 |
| collinear / repeated vertices | 2 | 0 | 2 | 0 |

**Verdict: PASS.** The 46/46 reproduction matches the proposal's table exactly: the naive
comparison calls every unit-determinant triangle collinear, the filter refuses to answer,
and the exact path recovers the sign. The exact-after-rounding control is wrong on the
same 46/46, which is the "must start from the original coordinates" claim, now with a test.

Filter selectivity on random input is 0 fallbacks in 20,000 cases — the filter answers
everything it can prove, and near-degenerate input is the only thing that reaches the
exact path. Cost of the whole predicate (filter + exact fallback + bound checks) was
**1.07x–1.16x** the naive comparison across runs (1.45–1.89 µs/op naive on CPython 3.12,
this host). H4's predicate half holds.

## 3. H2 — the runtime authority

Five misuse controls against the gate, plus the honest path as a positive control. The
"sign-only" column is what an ordinary robust-predicate library would apply.

| Control | Gate | Sign-only would apply | Correct |
|---|---|---:|---:|
| positive: same triple, same revision | accepted | +1 | +1 |
| wrong vertex order | rejected | +1 | −1 |
| coordinate edit, revision bumped | rejected | +1 | −1 |
| different mesh, same revision | rejected | +1 | +1 |
| coordinate edit **without** a revision bump | rejected | +1 | −1 |
| forged witness (direct construction) | rejected | — | — |

**Verdict: PASS**, with one finding the proposal does not have:

> **Revision-only invalidation is not enough.** The proposal invalidates on a coordinate
> edit; a coordinate write that bypasses the revision counter still produces a witness
> whose sign is wrong for the mesh as it now stands. Adding a digest of the classified
> coordinates to the witness closes it (row 5). The digest is 16 hex chars of SHA-256 over
> the triple's coordinates — 7.1 µs per mint on this host, dominated by hashing, not by
> the predicate.

This is a real cost: hashing is what makes the witness expensive relative to the
predicate, and it is only needed because the runtime cannot otherwise see a silent write.
In a language where the mesh's coordinate storage is private and all writes bump the
epoch (which is what the proposal asks for), the digest is redundant. **Recommendation:
drop the digest if the epoch is enforced, keep it if the gate must survive a bypass.**

## 4. H3 — the static rule, which the proposal explicitly did not model

`witness_checker.py` implements the rule over a tiny Hybrid-shaped IR:

```
observe(m, i, j, k)   : obs[m@e, (i, j, k)]
orientation(o)        : turn[m@e, (i, j, k)]
insert(m, i, j, k, t) : requires t : turn[m@e_now, (i, j, k)]
edit(m)               : e -> e+1
f(...) -> turn[m@m,(i, j, k)]   the only way a witness crosses a helper boundary
```

Ten cases, four of them positive controls (so a "reject everything" checker cannot pass):

| Case | Verdict | Code |
|---|---|---|
| 01 direct observe → commit | accept | — |
| 02 helper indexed by its own parameters | accept | — |
| 03 helper drops the indices (`-> turn`) | reject | `HYB-E4645` |
| 04 valid +1 for (0,1,2) offered to a write at (0,2,1) | reject | `HYB-E4644` |
| 05 edit between observation and commit | reject | `HYB-E4643` |
| 06 same vertices, different mesh | reject | `HYB-E4642` |
| 07 helper edits the mesh it was given | reject | `HYB-E4647` |
| 08 witness through two helper boundaries | accept | — |
| 09 caller keeps the bare sign | reject | `HYB-E4645` |
| 10 monomorphic helper (`-> turn[m@m,(0,1,2)]`) | accept | — |

**Verdict: PASS, with a condition the proposal is missing.**

1. **The rule needs indexed return types.** A helper that returns a witness must declare
   *which* mesh, which epoch and which ordered triple it is returning, and may name its
   own parameters (`turn[m@m,(i,j,k)]`), otherwise every helper is monomorphic (case 10)
   and the reusable version is impossible. That is dependent typing over three indices.
   It is the single biggest implementation risk in the proposal, and it is invisible in
   the Python model because Python constructors are public.
2. **New failure mode: a helper that edits the mesh it was given.** `@m` means "the
   caller's epoch at the call site"; a callee that calls `edit(m)` silently breaks that
   promise and the caller's stale check never fires. Detected here as an effect check
   (`HYB-E4647`), conservative: no `edit` of a passed mesh in a function that returns a
   witness. The proposal's design section does not mention this case, and its Python
   model cannot hit it.
3. **The epoch is per mesh, not per site** — editing an unrelated region of the same mesh
   kills reuse, as the proposal's own cost section concedes. Nothing here narrows that.

Cost: **35.2 µs/case** for parse + check over the ten cases (11.8–72.6 µs depending on
helper depth) in CPython. That number is not a compiler number and is not presented as
one; the useful signal is that checking is linear in statements with no fixpoint, no
alias graph and no interprocedural dataflow beyond a per-call signature substitution and
a one-level transitive effect walk.

## 5. What this spike does *not* establish

- **It never ran `hy-check`.** No Go/Rust toolchain was available on the spike host, so
  "the checker accepts this" is not demonstrated for any `.hy` file. The IR is a model of
  the rule, not the rule inside the real front-end; the cases in `cases/` are labelled
  `.hyc` for that reason.
- **No control flow, loops, generics, method receivers, closures, or `managed` blocks.**
  A `managed("...", "other")` block around the write is a permitted bypass under the
  current GUARANTEES row 8 — the rule as modelled would not see it.
- **The opaque-Go-API control is in section 8.** The proposal's own drop condition is now
  measured: a plain Go library reproduces the runtime guarantee. What it cannot reproduce
  is the compile-time half.
- **No soundness proof.** Indexed types over a real language need a progress/preservation
  argument; this is a prototype checker with tests, nothing more.

## 6. Cost, honestly

| Item | Measured / counted |
|---|---|
| Witness ceremony per write site | 2 lines (`o = observe(...)` / `t = orientation(o)`) — or 1 call to an indexed helper |
| Helper ceremony | 1 indexed return type + its three indices |
| Predicate overhead vs naive | 1.07x–1.16x |
| Witness mint (with digest) | 7.1 µs on CPython; dominated by hashing |
| Checker | 35.2 µs/case mean, no fixpoint |
| New diagnostics | 5 codes (`E4642`–`E4647`) |
| Go opaque API, full gate (section 8) | 380 ns/commit, 0 allocs; 21 ns without the digest |

## 7. Verdict for the ledger

**Feasible, cheap at runtime, and more expensive at the type level than the proposal
implies — and the proposal's own drop condition fires.** The numeric half reproduces
exactly and the runtime gate catches every control once a coordinate digest is added. The
language half is implementable but requires dependent indices on witness-returning helpers
and an effect check for callees that edit a mesh they were handed — neither is in the
proposal's design, and the second is a soundness hole in it. Section 8 then shows that a
plain Go library already delivers the runtime guarantee with no caller bookkeeping, which
leaves the feature's justification resting on the compile-time claim alone.

## 8. The opaque Go API control — the proposal's own drop condition

The proposal's falsification milestone ends with: *"If an ordinary opaque Go API gives the
same guarantees at the same boundaries without recomputation or caller bookkeeping, I would
drop the language feature."* That API exists, so it was written and measured.

`go-control/` is a two-package Go module (no dependencies, `go 1.22` directive):

- `mesh` — the opaque API. `Mesh`'s coordinate slice is unexported; `Observe` returns a
  `Turn` whose every field is unexported and whose validity depends on a package-held mint
  capability; `Insert` is the only write path and checks identity, epoch, ordered triple
  and a digest of the classified coordinates.
- `naive` — the sign-only control: same exact predicate, returns a bare `int8`, applies
  whatever it is told.

| Control (same four as section 3) | Opaque API | Sign-only |
|---|---|---|
| wrong vertex order | `ErrTriple` | applied |
| coordinate edit (epoch bump) | `ErrEpoch` | applied |
| other mesh at the same epoch | `ErrMesh` | applied |
| witness through a helper, reused for another triple | `ErrTriple` | applied |
| zero-value `Turn{}` (forgery) | `ErrForged` | n/a |
| coordinate write behind the epoch counter (in-package) | `ErrCoords` | n/a |

No caller bookkeeping: the caller holds a value and passes it. No recomputation: `Insert`
never re-runs the predicate, it compares a 128-bit digest. The guarantee is identical to
the modelled language rule at every boundary the rule covers — but it is a **runtime**
rejection, not a compile-time one.

### Cost, measured

`go1.27.4 windows/amd64`, i5-1035G4 @ 1.10 GHz, `-benchmem`, median of 3 runs, 0 allocs
across the board:

| Operation | ns/op |
|---|---:|
| naive float64 sign (`mesh.Sign`) | 1.0 |
| exact predicate, unit-determinant family (`classify`) | 74 |
| unchecked write (`m.epoch++`) | 7 |
| insert, epoch/triple/mesh checks only | 21 |
| insert, full gate with 128-bit digest | 380 |
| observe (predicate + digest + binding) | 480–553 |

Two things follow. The **authority checks themselves are ~21 ns** — about 3x an unchecked
write, which is noise in any real mesh edit. The **digest is 360 ns, i.e. 94% of the gate**,
and it is the part that makes the API's guarantee survive a write that bypasses the epoch
counter. SHA-256 over 48 bytes is the wrong primitive for this: an ordered per-triple
version stamp, or a 64-bit multiply-mix over the three coordinates, would keep the same
property for a few nanoseconds. That optimisation is not measured here and would be the
first thing to fix if the API route were chosen.

### What the delta actually is

The Go API matches the runtime guarantee, so the proposal's drop condition is met for the
runtime half. Three differences survive, and only the first is measured:

1. **Cost.** 380 ns per commit against 0 ns for a static rule — 94% of it the digest.
2. **Compile-time vs runtime.** The static rule rejects a *program*; the API rejects an
   *operation*. A caller that ignores the returned error does not corrupt the mesh (the
   write does not happen), so the failure mode is a silent no-op, not unsoundness.
3. **Closure under future code.** Nothing stops a colleague adding
   `func (m *Mesh) ForceInsert(...)`, or a second write path in another file; the API's
   guarantee is a property of today's code, and only review keeps it. A language rule is a
   property of every accepted program, including ones written next year. **This is the
   only argument for the feature that the control cannot answer — and it is an argument
   about enforceability, not about expressiveness.**

So: the proposal's premise ("a sign can be mathematically right and still authorize the
wrong edit") is true of *sign-returning* libraries and false of an opaque-witness library.
Under its own drop condition, the entry should be recorded as **learned: the runtime half
is available in Go today; the language feature must justify itself on the static half
alone.**

