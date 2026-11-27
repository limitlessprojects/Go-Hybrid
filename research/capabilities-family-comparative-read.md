# Capabilities family — comparative read for the 2026-12-01 window

**What this is.** The 2026-11-01 verdicts re-slotted W-001, W-002, W-003,
W-014, and W-017 together: "five spike artifacts (#336, #370, #381, #505, #516)
overlap on the alias-graph replacement; the family needs one comparative read
before any member can win." This file is that read. It covers the input set,
whether each artifact reproduces, what each one establishes, and one shared
matrix of the shapes that decide the family.

**Conflict of interest, stated first.** The author proposed W-003. Every claim
below is therefore tied to a command you can rerun. The matrix also shows where
W-003 loses or needs another member's machinery.

## 1. The input set needs two corrections

| Listed | What it actually is | Family artifact? |
|---|---|---|
| #336 | W-001 `spike11-region-capabilities`: Rust rule-algebra model + manual corpus mapping | yes |
| #370 | W-017 `spike12-operational-take-and-detach`: Rust model + Go `-race` harness | yes |
| #381 | W-014 `spike14-hyregion-slab-unification`: Rust model + Go `-race` harness | yes |
| #505 | Automated whole-repo PR titled "resolve #417" (W-020, STM regions); adds no `spikes/` directory | **no** |
| #516 | Automated whole-repo PR titled "resolve #375" (W-021, kill libhyrt.a); adds no `spikes/` directory | **no** |

The list also leaves out three artifacts:

- **W-003's executable spike.** PR #1071 was closed on 2026-10-24 by the queue-reset sweep ("not a verdict on the work"), so it never entered the window's input. It is superseded by `spikes/w003-generation-capability/` in this PR.
- **W-004's spike #507.** The window already pairs it with W-017's P4.
- **W-022's #545.** Its P4a falsifier is the same residual (see §4).

W-002 (#279) has no executable artifact. PR #2073 is a third-party W-003
simulation written in Python, with no Go. It is 995 commits behind
main and conflicted. It is noted here and not used.

## 2. Every artifact reproduces at its own head

Commands were run at each PR head under Go 1.24.7, after setting `go.mod` to
`go 1.24` locally where the spike declared 1.26. No spike used a newer
language feature. Rust models were built with `rustc --edition=2021 --test`.

| Artifact | Go `-race` suite | Rust model |
|---|---|---|
| #336 W-001 | — | 6/6 pass |
| #370 W-017 | pass; each of its 5 `HY_NEG` controls races, as documented | 12/12 pass |
| #381 W-014 | pass | 10/10 pass |
| #507 W-004 | pass | — |
| #545 W-022 | pass | — |
| this PR W-003 | pass, `-count=5`; 6/6 controls race 3/3 | — |

## 3. What each artifact establishes

Each entry gives the evidence class first.

- **W-001 (#336), model only.** By its own verdict the result is
  *inconclusive*. Region capabilities can express transfer, but region
  provenance still has to flow through aggregates, calls, closures, and joins.
  In the author's words, the model "relocates rather than removes" value flow.
  The 24-case executable encoding and the 250-LOC target were not met.
- **W-017 (#370), executable, race-verified.** This is the strongest pre-window
  artifact. Operational `__hy_take` costs 0.30 ns and 0 allocs. Capacity
  clipping and selective detach close the #38/#98/#206 chains. Its own P4 shows
  that a missed alias, closure, or defer still races on the fast path, and that
  a read through a zeroed map succeeds silently.
- **W-004 (#507), executable.** Zero-size witnesses move E3003 into Go's own
  type checker at 0 cost, for the non-captured subset only. P4 fired: captures
  keep sharing storage.
- **W-014 (#381), model plus a Go harness for slabs.** Static 1-bit
  union-find unification of region roots, with off-heap slabs addressed by
  `SlabHandle{SlabID, Epoch}`. Its Go harness checks an **epoch word on every
  access and CASes it on transfer**. That runtime primitive matches W-003's
  corrected design (§4), applied to off-heap slabs rather than Go-heap
  composites. The static unification over-merges containers, which W-015's
  `split` handles. No measured corpus encoding exists.
- **W-003 (this PR), executable, race-verified, pre-registered.** A generation
  word on the storage: one atomic load per use, a 7 ns CAS per move, 0 allocs.
  It fails closed on captured, missed-alias, and defer-held uses, whichever
  binding they come through. It also carries two corrections to its own
  proposal: the mark is a generation word, not one bit (B1 is falsified by
  ABA), and the header-padding claim is withdrawn.
- **W-002 (#279), no artifact.** It is the static half of the same idea:
  `own`/`share`/`borrow` constructors, i.e. W-003 part (a) without the runtime
  mark.

## 4. The shared matrix

Shapes come from the family's own falsifiers. Cells cite the test that shows
the behaviour. "fails closed" means a `hyb: use of moved value` panic before
any access to storage.

| Shape | W-017 take | W-004 witnesses | W-001 / W-014 static regions | W-003 generation word |
|---|---|---|---|---|
| S1 captured use after move (#507 P4) | write: Go nil-map panic; read: silent zero (`TestTake_S1_…`) | races (#507 P4) | rejects **if** closure region summaries are inferred (#336 case 36; #381 model) | **fails closed** (`TestG_S1_…`) |
| S2 missed alias (#370 N0) | races (`TestNeg_Take_S2_…`) | races | rejects **if** the alias edge is inferred | **fails closed** (`TestG_S2_…`) |
| S3 defer arg before move (#370 E1) | races (`TestNeg_Take_S3_…`) | races | rejects if defer args join the region | **fails closed** (`TestG_S3_…`) |
| S4 stale after two moves | silent | compile error (non-captured) | rejects (moved bit set) | **fails closed**; literal 1-bit variant is silent |
| S5 raw/interior view escapes (= #545 P4a) | races | races | needs a view/projection rule | **races** — needs the same static rule |
| Cost per use / per move | 0 / 0.3 ns | 0 / 0 | 0 / 0 (compile time) | 1 load / 7 ns |

**How to read it.** Every static member is exactly as strong as its alias
recall: each "rejects if" is a value-flow inference that must not miss. The
runtime generation word does not depend on recall for S1–S4. It cannot see S5,
and neither can anything else without a static rule. So S5 is the true common
core of the family. #545 reached the same wall from the pool side (P4a).

## 5. What the family decomposes into

These five proposals are not five rival replacements for the alias graph.
Their verified parts fit together:

1. **A local static rule for raw and interior views** (S5 / P4a). A `view`
   or projection may not be stored, captured, or kept across a `move`. This is
   the irreducible static piece. W-002 and W-003 (a) state it, and W-022 R1
   needs it.
2. **A runtime generation word on storage.** It makes every stale binding
   fail closed regardless of alias recall. Evidence: this PR on Go-heap
   composites, #381 on off-heap slabs, and the shipped Rust handles
   (`runtime/rust/hyrt/src/lib.rs`, `handle_pack(idx, gen)`).
3. **Cheap operational transfer** (W-017 take plus capacity clip) for the
   un-aliased fast path. It is compatible with (2): take the binding *and*
   advance the generation.
4. **Zero-cost compile-time witnesses** (W-004) for the non-captured subset.
5. **Static region inference** (W-001, W-014). By #336's own verdict it is
   still value flow, so it adds recall but not a new guarantee class.

**The open question for the window** is which member contributed a verified
piece that no other member has. For (2) on ordinary Go composites, with the
family's own falsifiers as its test set, that member is W-003. For (3) it is
W-017. The maintainer decides; this read only makes that question answerable.
