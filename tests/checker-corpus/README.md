# tests/checker-corpus — adversarial programs with pre-registered verdicts

**Current registered corpus: 130 programs — ACCEPT 40 · ACCEPT-MANAGED 2 · REJECT 88 — each with a machine-readable `// EXPECTED-VERDICT:` header and a
**Current registered corpus: 130 programs — ACCEPT 40 · ACCEPT-MANAGED 2 · REJECT 88 — each with a machine-readable `// EXPECTED-VERDICT:` header and a
row in [VERDICTS.md](VERDICTS.md). The 18 originally registered here were
written BEFORE the Phase-1 checker existed; that registration grew to 20
during the 2026-10-12 integration, and every case after it was added
additively, still hand-derived from the rule text before it was run.

Two additive registrations are worth naming because they moved cases around.
The 2026-10-14 select-send registration (mutable-map rejection, a positive
string/channel control, and fail-closed rejection of an unresolved name)
landed as corpus **32–34** after a renumber, and a further case pinned the
documented default-arm syntax and its nonblocking fallback printing exactly
`idle`. Those four files were initially numbered 21–24 and then 25–28 on the
contributor's branch; the 2026-10-15 maintainer integration renumbered them
to 32–35 (slots 25–31 were taken by the audit/PR wave) without changing
their expected results. Their expected results were recorded before running
the new fixtures.

The prose below is the original pre-registration statement, kept as written
on its date: the corpora sizes it names (18, 20) are what the corpus held
then, not today's 101. The dated tripwire baseline is preserved as historical
for the same reason.

18 pseudo-Hybrid (`.hy`) programs written BEFORE the Phase-1 checker exists,

per the human owner's 2026-10-12 ruling (QUESTIONS.md Q1): the adversarial
programs are written first and treated as the spec. Coverage: the named
tripwire cases (shared map across tasks; worker pool mutating a shared
struct — unguarded, mutex-guarded, managed shapes), the three G0-named
adversarial programs (shared-mutable-map, sender-keeps-using-after-send,
borrow-returned-from-function), rule exercises for A2/A3/A5/A6/A7, and
positive controls guarding against over-strictness (per-task fan-in, clean
`move` sends, atomic/channel/immutable sharing). VERDICTS.md has the table.

Pre-registration discipline: every verdict was hand-derived from the (c)+(b)
sendability ruling and the spike-6 A1–A8 strict readings before any checker
existed. Verdicts change only by logged amendment in VERDICTS.md — never
silently. A checker/corpus disagreement is a bug in one of them: decide
which, record it, never quietly re-run until green.

How the future checker consumes this: each `.hy` header carries one
machine-readable `// EXPECTED-VERDICT:` line (ACCEPT / ACCEPT-MANAGED /
REJECT <rule-id>) plus human-readable reasoning (value layers, check
sites, loan windows). The Phase-1 harness parses the line, runs the
checker, and diffs; for ACCEPT-MANAGED the managed LINT output is part of
the expected result. Contested programs say so in the line's trailing
note, so they can be pinned separately from settled ones.

Tripwire rule: more than ~25% of this corpus needing `managed` reopens G0
(QUESTIONS.md Q1 item 5). Baseline at pre-registration: 11.1%
verdict-level / 16.7% as-written — definitions and recompute rules in
VERDICTS.md §Tripwire baseline.

`78-shadowed-unused-import-reject.hy` — filacocta (PR #535): an explicit
import whose only apparent use names a shadowing local must still reject
as HYB-E1002 (the emitted Go would carry an unused import). Behavior
landed on main via the #475/#430 buildable work; the fixture pins the
surface with author credit (tests extracted to
`hy-check/tests/import_shadowing_guards.rs`).

With this registration the corpus is **78 programs: ACCEPT 21 ·
ACCEPT-MANAGED 2 · REJECT 55**.

`control-*.hy` — isHeSatoshi (PR #598): seven **negative controls** for the
accept-side oracle wave. The Go-toolchain oracle (#577) measured a 38%
accept-side residual on generated programs and named ten accepted-but-
unbuildable classes to fix; fixing those means adding range, bounds and shape
checks to more contexts, and every check added can over-reject. A corpus that
only grows the REJECT side cannot detect that. Each control is ACCEPT today
**and builds under `go build`**, verified before commit — an accept-side
control that does not build would pin the wrong thing. Four fence off-by-ones
in the const-overflow wave (the exact int8 edges `127`/`-128` in one file, the
unsigned `255` edge, a folded constant expression `100 + 27`, and the
in-range twin of #519's composite-literal overflow). Three are legal Go the
wave must not confuse with the array form: `xs[300]` on a **slice** compiles
and panics at run time while `a[300]` on `[2]int` does not, `-n` on an int8
**variable** wraps at run time because Go does not fold it, and
`var a [300]int8` bounds an array **length** by a count, not by the element
type's range. Each header states what it fences and cites GUARANTEES.md Row 13
(BUILDABLE) or the Row 16 "No claim" position for run-time panics, so a
reviewer can see why ACCEPT is correct rather than an oversight.

With this registration the corpus is **89 programs: ACCEPT 29 ·
ACCEPT-MANAGED 2 · REJECT 58**.

`*-addr-of-call-*.hy` — G0st13 (PR #436): the corpus-side pin for the
address-of launder (#435). `collect_ref_roots` walks `AddrOf` and the
`UnOp::Addr` spelling, so `p := keep(&h)` contributes its root and `move h`
freezes it — the escape rejects with `HYB-E3003`. The ACCEPT control keeps a
read-only `read(&h)` (address taken, scalar returned) legal so the rule
cannot degrade into "never take an address". Unit tests carried in
`hy-check/tests/addr_of_alias.rs` (reject, the direct `&h` twin, both
controls). Registered as 65/66: the PR authored them as 61/62, but 61–64
were taken before it landed.

`conversion-arity-reject.hy`, `struct-literal-unknown-field-reject.hy`,
`field-assign-wrong-type-reject.hy`, `struct-and-conversion-controls-accept.hy`
— isHeSatoshi (issue #682): the U13–U16 accept-but-unbuildable type/shape gaps.
Each of the three rejects was reproduced as a real `go build` failure against
the emitted Go before its fixture was committed, and all three are
`HYB-E1002` (`BUILDABLE`, Row 13). The fourth fixture is the ACCEPT half and is
the reason the batch is safe to land: it holds the legal neighbours the new
checks must not touch — a one-argument conversion, a variadic builtin's own
arity, declared struct fields including a subset, an untyped int constant
assigned to a `float64` field (which is why `Int` into a float target must stay
allowed), a same-typed struct assignment, an `any` target, and a multi-value
binding that is not a single-target assignment. Rules live in
`hy-check/tests/struct_and_conversion_shape_guards.rs` (8 tests).

`any-arithmetic-reject.hy`, `any-field-access-reject.hy`, `any-index-reject.hy`,
`any-call-reject.hy`, `any-operations-controls-accept.hy` — isHeSatoshi
(issue #682, the same report as the U13–U16 batch above): the U9–U12 operations
on `any`. The root cause was a classification rather than a missing check: `any`
resolved to `OpKind::Unknown`, and `Unknown` is fail-open everywhere by design
(it means "the checker could not resolve this"). `any` is the opposite — a
resolved type Go refuses to operate on — so it now resolves to
`OpKind::Interface`, which puts it in the operand checks' reject set because
those checks list the kinds they ACCEPT. All four rejects are `HYB-E1002`
(`BUILDABLE`, Row 13) and each was reproduced as a real `go build` failure
first. The fifth fixture is the ACCEPT half and carries the weight here: `any`
is legal as an assignment target, a call and task-spawn argument, a container
element and map value, a struct field type, and an `==` operand (Go converts the
non-interface side, so only the ORDERED comparisons are illegal).
`fmt.println(v)` is pinned explicitly because it is a Field access whose base is
the `fmt` package rather than an interface — the first false positive a careless
field-access check produces. Rules live in
`hy-check/tests/any_interface_operations.rs` (7 tests).

With this registration the corpus is **100 programs: ACCEPT 32 ·
ACCEPT-MANAGED 2 · REJECT 66**.
`67-adversarial-race-spawn.hy` — KonstantinPythonLab (PR #402, issue #5):
the task-spawn-boundary adversarial — a closure that WRITES a captured
mutable binding, spawned as a task, with main still reading the binding
after the spawn. The maintainer integration translated the PR's pseudo-
syntax (`func main`, `var x: int = 42`, `spawn fn(){}()`, `print`) to
Hybrid's actual grammar (`fn main`, `x := 42`, `task fn() { ... }()`,
`fmt.println("{x}")`) with the adversarial intent unchanged; the checker
rejects at [S1] with A1c/HYB-E3001 (the spawn-callee proof: closures share
every capture with the new task).

With this registration the corpus is **101 programs: ACCEPT 32 ·
ACCEPT-MANAGED 2 · REJECT 67**.

`multivalue-in-single-value-reject.hy`, `multivalue-arg-arity-reject.hy`,
`void-as-value-reject.hy`, `multivalue-and-void-controls-accept.hy` —
isHeSatoshi (issue #682): the U6–U8 value-position classes. All three share one
root cause — `kind_of_expr` on a call returned the FIRST result kind for a
multi-value call (silently discarding the rest) and `OpKind::Void` for a
no-result call, which no operand check rejected. The fix derives the call's
value count in one place (`non_single_value_count`) and rejects it in the
positions that need exactly one value. The `void-as-value-reject.hy` header
records that this class was already latent in the repo: the T22 fixture in
`hy-check/src/lib.rs` used `is_ok(delete(alias, "k"))` to get a `delete` into an
`if` condition, and `delete` produces no value either, so that fixture was
itself accepted-but-unbuildable. The new rule surfaced it; the fixture was
rewritten to put the `delete` in a closure body with the closure called in the
condition, and both of its assertions stayed strict. The fourth fixture is the
ACCEPT half and is the reason this batch is safe: it holds every legal
multi-value form — `a, b := two()`, `a, b = two()`, `return two()` with matching
results, `g(two())` where `g` really takes two parameters (which is what makes
the rule an arity comparison rather than a blanket rejection), a no-result call
as a statement and as a `defer` operand, the two non-call two-value sources
(`m[k]` and `<-ch`), and single-value calls in binding, operand, and argument
positions. On the merged tree the U6 operand fixture's leading diagnostic is
the landed T23 rule's `HYB-E2002` (pr-599/#669 classifies the containing
1-name binding first), with the new rule's `HYB-E1002` co-emitted; the fixture
header records this and the rust test pins E1002's presence. Rules live in
`hy-check/tests/multivalue_and_void_positions.rs` (8 tests).

With this registration the corpus is **109 programs: ACCEPT 34 ·
ACCEPT-MANAGED 2 · REJECT 73**.

`undefined-member-call-reject.hy`, `defer-arity-reject.hy`,
`append-element-type-reject.hy`, `member-defer-and-append-controls-accept.hy`
— the #682 U2–U5 buildable wave (issue #682 group C, pr #707 by isHeSatoshi).
Four `hyb check`-accepted programs whose emitted Go failed `go build`: a call
to a member that does not exist (on a scalar and on a struct), a deferred call
with the wrong arity, and `append` with an element of the wrong type. Each was
reproduced as a real `go build` failure before the check was written, and each
rejection is paired against a legal neighbour inside the single ACCEPT control
— the ordinary method/field cases, `stack.pop()` (a `hy-emit` intrinsic lowered
to `hy_pop(&stack)`, not a Go method), `*atomic.int64` members (a foreign
generic whose members the kind model cannot see, so the rule must fail open),
`defer` on a builtin and on a method, and `append` with a variadic spread,
multiple values, and an untyped int constant into a `[]float64`. Regression
pins live in `hy-check/tests/member_defer_and_append_guards.rs` (9 tests).
With this registration the corpus is **115 programs: ACCEPT 36 ·
ACCEPT-MANAGED 2 · REJECT 77**.

`typed-const-expression-overflow-reject.hy`,
`typed-const-expression-controls-accept.hy` — issue #725: a **typed constant
expression whose folded result overflows its type**. Go folds constant
expressions before the range check, so `u8(255) + 1` is `constant 256 of type
uint8 overflows uint8`; `hyb check` accepted it with 0 diagnostics and the
emitted Go failed `go build`. The per-operand check could not see it — 255 fits
`u8` and 1 fits `u8`, so both passed and the **result** was never computed —
and shifts took an early return that skipped every range check.

Two halves of the fix are needed together, which is why the reject fixture is a
single file with eight expressions rather than eight files: (1) the constant
folder gained an `Expr::Call` arm so `u8(255)` folds to a constant at all, and
(2) the result is folded and range-checked against the typed operand's type,
shifts included, plus a negation check. Without (1) a result check sees
`NotConst` and stays silent, so a fix that skipped it would look correct and
catch nothing.

The accept control is the load-bearing half: SPEC X1/X2 define run-time
overflow as **wrapping**, so `var x u8 = 200; var y u8 = 100; x + y` must keep
building. The control also pins SPEC X4's own example (`u8(300)` is 44), the
in-range typed forms, and a comparison (which yields a bool, not a `u8`).
Regression pins live in `hy-check/tests/typed_const_expression_overflow.rs`.

HYB-E6002 gained the `tier0_rule_id: "BUILDABLE"` join key it was missing (it
already declared `"rule": "BUILDABLE"`, so the corpus could only have said
`<unclassified-error>` for it before).

## Bounty-track registration — 2026-10-19: constant index past a fixed array's length (BUILDABLE, Row 13)

`check_index_kind` bounded the constant index of the STRING form (`s[10]` on
`const s = "abc"`) but not the fixed-array form: `a[5]` on `[3]int` checked
clean (0 diagnostics) and the emitted Go then failed `go build` with
`invalid argument: index 5 out of bounds [0:3]`. The rule now reads the array's
declared length (through alias resolution) and compares the folded constant
index against it.

| program | verdict | rule | diagnostic | rationale |
|---|---|---|---|---|
| array-const-index-past-length-reject | REJECT | BUILDABLE | `HYB-E1002` | `var a [3]int` then `a[5]`: a constant index past a fixed array's length. The length lives in the type, so `var`/`let`/`:=` bindings and `[3]int` parameters are bounded as well; the fixture's own `a[2]` lines stay clean, and `control-slice-index-past-length` keeps the slice form ACCEPT — a slice's length is not part of its type, so no constant bound is decidable there |

The rule lives in the array branch of
`hy-check/src/buildable.rs::check_index_kind` (over `const_array_len_at`),
pinned by `hy-check/tests/array_const_index_bounds.rs` (8 tests: the boundary
`a[3]`, the typed-literal and parameter forms, the string sibling, and the
in-bounds / slice / long-array controls). The reject was reproduced as a real
`go build` failure on the pre-fix tree, and `a[2]`, `xs[300]` and `[300]int8`
were verified ACCEPT, before this fixture was committed.

Reproduction (branch tree, pinned rustc 1.99.0 / go1.26.9):
`python3 tools/corpus_harness.py --bin compiler/target/debug/hyb` reports
**116/116 verdict agreement, tripwire not fired**.

With this registration the corpus is **116 programs: ACCEPT 36 ·
ACCEPT-MANAGED 2 · REJECT 78** — main's 115 plus this one.

## Bounty-track registration — 2026-10-19: map-slot borrow as a call argument (LOAN-ESCAPE, Row 13)

O3 spells the map slot loan as `n := &m[k]`: the borrow is the whole value of a
binding, and that is the shape the emitter lowers to a copy-out / store-back.
In an argument or operand position there is no binding to store back to, so the
`&m[k]` was emitted as written and `go build` refused the program:

```
invalid operation: cannot take address of e.services["a"] (map index expression)
```

The checker accepted it, so the program was an accept with no lowering — the
differential loop cannot reach a running binary from it. The rule now walks the
whole expression in every position and reports HYB-E3104 unless the borrow is
the root of a binding initializer or a re-binding assignment; parentheses are
position-transparent (`n := (&m[k])` is a loan exactly like `n := &m[k]`).

| program | verdict | rule | diagnostic | rationale |
|---|---|---|---|---|
| map-slot-borrow-callarg-reject | REJECT | LOAN-ESCAPE | `HYB-E3104` | `touch(&e.services["a"])`: a map-slot borrow in argument position has no lowering, and the emitted Go does not build. The shape test is on the MAP (an index expression over a map is not addressable in Go) — slice and array elements (`&xs[0]`, `&a[0]`) are addressable and keep their lowering, so the rule cannot widen into those |
| map-slot-borrow-paths-accept | ACCEPT | — | 0 diagnostics | the legal spellings in one program: `n := &e.services["a"]`, `m := (&e.services["b"])`, their store-backs, and the corrected call-argument pattern `tmp := e.services["a"]; touch(&tmp)`. Built and run (prints `3 4 7`) so the new rule cannot refuse a buildable program |

The rule lives in `hy-check/src/addr_loan.rs` (`walk_addr_of_loan_projections`
plus `map_slot_borrow_span` / `emit_map_slot_borrow_diag`), pinned by
`hy-check/tests/slot_loan_callarg_positions.rs` (7 tests: the map-field and
local-map argument forms, the argument borrow that overlaps a live loan, the
base-store control, both legal binding positions, the parenthesised binding,
the corrected copy pattern, and the array-element control). The reject was
reproduced as a real `go build` failure first, and the accept fixture was
checked, built and RUN before it was committed.

Reproduction (branch tree, pinned rustc 1.99.0 / go1.26.9):
`python3 tools/corpus_harness.py --bin compiler/target/debug/hyb` reports
**118/118 verdict agreement, tripwire not fired**; `tools/differential_loop.py`
reports **39/39 accept-side (build+run) + 79/79 reject-side**.

With this registration the corpus is **118 programs: ACCEPT 37 ·
ACCEPT-MANAGED 2 · REJECT 79** — main's 116 plus these two.

`type-name-in-value-position-reject.hy`,
`type-name-in-value-position-controls-accept.hy` — issue #778: a **type name
used where a value is required**. `x := holder` for a `struct holder` passes
`hyb check` and the emitter prints the identifier verbatim, so `go build` fails
with `holder (type) is not an expression`. Four value positions, plus a built-in
type and a type alias. Nothing runs, so this is claim-precision (Row 13), not a
soundness hole.

The rule has to be narrow, and the accept control carries that weight: a type
name is legal as a conversion CALLEE (`holder(1)`, `int(3.0)`) and as the FIRST
argument of `new`/`make` (`new(bag)`), and a local binding shadows it. `new(bag)`
is the control that matters most — an early version of the check walked that
argument as a value and broke `n5-zero-map-first-write.hy`. The corpus caught it
on the first run, which is exactly why the fixture pins it. Regression pins live
in `hy-check/tests/type_name_in_value_position.rs` (13 tests).

With this registration the corpus is **120 programs: ACCEPT 38 ·
ACCEPT-MANAGED 2 · REJECT 80** — main's 118 plus these two.
