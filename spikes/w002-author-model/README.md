# W-002: author clarification and executable capture fragment

Proposal: [#279](https://github.com/limitlessprojects/Go-Hybrid/issues/279),
ledger W-002, review slot 2. This is an isolated experiment, not a replacement
for `hy-check`. It changes no production compiler or corpus verdict.

## Answer to review question 1: closure capture

An ownership type must carry resource identity and capture effects, not merely
the outer spelling `own[T]`. Otherwise a closure can hide a live borrow.

The admitted fragment uses the following rules:

| Operation | Result / obligation |
|---|---|
| fresh allocation | infer `own[T, r]` with a fresh resource identity `r` |
| scalar or immutable literal | infer freely copyable `scalar` / `share[T]` |
| move an owner | consume the source binding; reject while any read loan of `r` is live |
| read borrow | produce `borrow[T, r]`; owner reads remain allowed, writes and transfers do not |
| closure capture by move | consume at construction, preserve the captured capability unchanged |
| closure capture by read borrow | keep a lexical loan live until closure call/drop |
| local closure call | consume this affine closure; actual body checking is future work |
| spawn closure | consume it; every recursively captured value must be owned or immutable, with no borrowed capture |
| duplicate capture / invalid capture | reject without changing the context |

In particular, `move` of a `borrow` into a closure does NOT turn it into
ownership. Recursive capture inspection rejects the nested-closure version
too. Read-loan tracking includes values stored in closure types, so moving the
borrow's binding cannot release the owner's obligation prematurely.

This deliberately conservative fragment treats every closure as affine.
Supporting repeatable closures requires an additional checked effect: no
consumption of an affine capture on any invocation. Supporting asynchronous
borrows requires a lifetime/region proof and structured joining. Neither is
silently admitted here. Function bodies must be checked in their own context
with the same transfer rules; the current API does not parse or verify bodies.

## Answer to review question 2: inference and compatibility

The examples require no user-written capability type annotations. Allocation
and literal constructors synthesize capabilities; explicit move and borrow
operations state intent. Unknown summaries reject. This establishes only local
inference for this fragment, not Hybrid's current zero-annotation accept rate.

There is a concrete compatibility cost: an unannotated assignment copying an
owned mutable object cannot silently produce a second owner. It must become a
borrow, a move, or an immutable share; the checker must derive and validate
that choice or request clarification. Automatic region/lifetime inference,
function summaries, control-flow joins and loop fixed points remain work.

The original proposal overstated how much reasoning disappears. This model
does not use a mutable alias/may-share graph, but it still stores resource
identity and lexical state in the typing context. Types alone do not eliminate
identity, lifetime, effect, or control-flow reasoning. The full graph-deletion
claim remains unproven. A failure to preserve accepted source programs without
reintroducing those obligations would be a publishable limit of the proposal.

The original proposal also mixed 'preserve diagnostic codes' and 'renumber
diagnostics'. Correction: preservation is a future compatibility target to be
measured, not a result or promise of this spike. No diagnostic is renumbered by
this change; this API uses its own explanatory rejection strings.

## Validation

The hypotheses and boundaries were committed before execution in
`PREREGISTRATION.md`, commit `eb28990`.

From the repository root, with Python 3:

```text
python -m unittest discover -s spikes/w002-author-model -p test_model.py -v
python spikes/w002-author-model/run_mutations.py
python tools/check_test_registration.py
```

Actual local results (Windows, Python 3.14.7, 2026-11-02):

- 20 semantic tests pass, including rejected programs and accepted lifetime-end
  controls, duplicate-capture atomicity, immutable copy and affine invocation.
- Both deliberately defective mutants are caught by existing semantic tests:
  removing the live-borrow transfer guard, and removing duplicate-capture
  rejection. These are not claims about detecting arbitrary defects.
- Existing Python test-registration guard passes. CI explicitly runs both the
  model tests and mutation runner.

No Rust/Go compiler is available in this local session. The workspace,
checker corpus, differential runner, actual Go execution and hosted CI have
NOT been run for this artifact. There is no corpus-parity percentage or
production speed/size claim. See `PREREGISTRATION.md` for the omitted forms.

## Relationship to independent implementations

[#1078](https://github.com/limitlessprojects/Go-Hybrid/pull/1078) by Rohank143
and [#1893](https://github.com/limitlessprojects/Go-Hybrid/pull/1893) by
harmweber787-png independently supply W-002 experiments. This artifact does
not claim authorship of those implementations, reproduce their reported test
counts, or redirect their payment details. Proposal credit and implementation
credit should remain distinguishable and any award allocation is the
maintainer's decision. No payment address is supplied here.
