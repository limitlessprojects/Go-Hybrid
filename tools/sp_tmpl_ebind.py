#!/usr/bin/env python3
"""sp_tmpl_ebind.py — T-EBIND binding-shape template family (bake-off arm A0,
round-2 class: #929 / #905 / #903).

CLASS-LEVEL transplants of already-landed fix patterns ONLY:
  * #926 count-guard shape (614f18b, buildable.rs): reject a value count the
    source cannot yield — here extended from call sites to EVERY binding
    site: parallel declarations/assignments with mismatched counts, and
    1-result sources bound to 2+ names.
  * #909/#969 kind-compat shape (c9edb09 / c203eca): positional kind
    compatibility via `kinds_conflict`, with the same conservative fail-open
    (Unknown/Interface, the untyped-int-to-float adaptation) the accept-side
    corpus pinned for those landings.
  * #929's own spec: a channel receive and a map index yield at most two
    values (`v, ok`); slice/array/string indexing yields exactly one — the
    binding-count rules live where T23 lives (tuple.rs), emitting HYB-E2002;
    the parallel/count/type rules live in buildable.rs, emitting HYB-E1002.
  * O8 slot-loan fail-open (`q = &m["a"]` into a value-typed q) is preserved
    per-pair in the positional parallel-assign check.

Variant routing (class membership, not hand-tailoring — every variant is the
same class fix; an issue outside the class must BLOCK):
  issue #929 names tuple.rs/T23            -> variant "tuple-guards"
  issue #905/#903 name buildable.rs sites  -> variant "buildable-guards"

Internal pre-verification (deterministic, disclosed in the rationale):
apply candidate -> cargo build -p hyb -> repro REJECT + expected code ->
accept controls ACCEPT -> emitted Rust tests pass -> corpus harness ->
census check -> only then propose. The harness gates re-verify independently.
"""

from __future__ import annotations

TUPLE = "compiler/crates/hy-check/src/tuple.rs"
BUILDABLE = "compiler/crates/hy-check/src/buildable.rs"

TUPLE_TEST_PATH = "compiler/crates/hy-check/tests/binding_shape_guards.rs"
TUPLE_TEST_STEM = "binding_shape_guards"
PAR_TEST_PATH = "compiler/crates/hy-check/tests/binding_shape_guards_parallel.rs"
PAR_TEST_STEM = "binding_shape_guards_parallel"

HELPERS = '''use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("parses");
    check_file(&ast)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn assert_code(src: &str, want: &str) {
    let cs = codes(src);
    assert!(
        !cs.is_empty() && cs.iter().all(|c| c == want),
        "expected only [{want}], got {cs:?} for:\\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(cs.is_empty(), "expected 0 diagnostics, got {cs:?} for:\\n{src}");
}
'''

TUPLE_TEST_SRC = (
    '''//! Binding-shape guards (T23 source counts) — #929 class.
//!
//! A channel receive and a map index yield at most two values (`v` or
//! `v, ok`); indexing a slice, array, or string yields exactly one and has
//! no comma-ok form. Binding a count the source cannot yield — including a
//! 1-result call bound to 2+ names, and a select receive arm with 3+ names
//! (which crashes the emitter's select lowering) — fails go build with
//! `assignment mismatch` and is rejected fail-closed before emission. Every
//! valid 1-value and comma-ok 2-value form stays legal.

'''
    + HELPERS
    + '''
#[test]
fn one_result_call_to_multiple_names_reject() {
    assert_code(
        "fn one() int {\\n    return 1\\n}\\nfn main() {\\n    a, b := one()\\n    _ = a\\n    _ = b\\n}\\n",
        "HYB-E1002",
    );
    assert_code(
        "fn one() int {\\n    return 1\\n}\\nfn main() {\\n    var a int\\n    var b int\\n    a, b = one()\\n    _ = a\\n    _ = b\\n}\\n",
        "HYB-E1002",
    );
}

#[test]
fn receive_over_two_names_reject() {
    assert_code(
        "fn main() {\\n    ch := make(chan int, 1)\\n    a, b, c := <-ch\\n    _ = a\\n    _ = b\\n    _ = c\\n}\\n",
        "HYB-E2002",
    );
    assert_code(
        "fn main() {\\n    ch := make(chan int, 1)\\n    var a int\\n    var b int\\n    var c int\\n    a, b, c = <-ch\\n    _ = a\\n    _ = b\\n    _ = c\\n}\\n",
        "HYB-E2002",
    );
}

#[test]
fn map_index_over_two_names_reject() {
    assert_code(
        "fn main() {\\n    m := map[string]int{\\"k\\": 1}\\n    a, b, c := m[\\"k\\"]\\n    _ = a\\n    _ = b\\n    _ = c\\n}\\n",
        "HYB-E2002",
    );
}

#[test]
fn slice_index_multi_name_reject() {
    assert_code(
        "fn main() {\\n    xs := []int{1, 2}\\n    a, b := xs[0]\\n    _ = a\\n    _ = b\\n}\\n",
        "HYB-E2002",
    );
    assert_code(
        "fn main() {\\n    xs := []int{1, 2}\\n    var a int\\n    var b int\\n    a, b = xs[0]\\n    _ = a\\n    _ = b\\n}\\n",
        "HYB-E2002",
    );
}

#[test]
fn select_recv_three_names_reject() {
    assert_code(
        "fn main() {\\n    ch := make(chan int, 1)\\n    select {\\n    a, b, c := <-ch:\\n        fmt.println(a)\\n    }\\n}\\n",
        "HYB-E2002",
    );
}

#[test]
fn valid_source_forms_stay_legal() {
    // 1-value and comma-ok 2-value forms (issue accept controls)
    assert_ok(
        "fn main() {\\n    ch := make(chan int, 1)\\n    v := <-ch\\n    _ = v\\n}\\n",
    );
    assert_ok(
        "fn main() {\\n    ch := make(chan int, 1)\\n    v, ok := <-ch\\n    _ = v\\n    _ = ok\\n}\\n",
    );
    assert_ok(
        "fn main() {\\n    m := map[string]int{\\"k\\": 1}\\n    v := m[\\"k\\"]\\n    _ = v\\n}\\n",
    );
    assert_ok(
        "fn main() {\\n    m := map[string]int{\\"k\\": 1}\\n    v, ok := m[\\"k\\"]\\n    _ = v\\n    _ = ok\\n}\\n",
    );
    assert_ok(
        "fn main() {\\n    xs := []int{1, 2}\\n    v := xs[0]\\n    _ = v\\n}\\n",
    );
    assert_ok(
        "fn main() {\\n    ch := make(chan int, 1)\\n    select {\\n    v, ok := <-ch:\\n        fmt.println(v, ok)\\n    }\\n}\\n",
    );
    assert_ok(
        "fn main() {\\n    ch := make(chan int, 1)\\n    select {\\n    v := <-ch:\\n        fmt.println(v)\\n    }\\n}\\n",
    );
    // 2-result call to 2 names stays legal (the T23 baseline)
    assert_ok(
        "fn minmax(a int, b int) (int, int) {\\n    if a > b {\\n        return a, b\\n    }\\n    return b, a\\n}\\nfn main() {\\n    lo, hi := minmax(3, 9)\\n    _ = lo\\n    _ = hi\\n}\\n",
    );
    // 1-result call to 1 name stays legal
    assert_ok(
        "fn one() int {\\n    return 1\\n}\\nfn main() {\\n    x := one()\\n    _ = x\\n}\\n",
    );
}
'''
)

PAR_TEST_SRC = (
    '''//! Binding-shape guards (parallel counts + declared-type compat) — #905/#903 class.
//!
//! A parallel declaration/assignment whose name and value counts differ
//! fails go build with `assignment mismatch`; a provably single value bound
//! to several names likewise. A typed declaration's initializers must
//! satisfy the declared type, and a parallel assignment is positional —
//! each pair must satisfy the same kind check as the single-target form
//! (with the O8 slot-loan fail-open preserved). Valid counts and adaptive
//! initializers stay legal.

'''
    + HELPERS
    + '''
#[test]
fn parallel_decl_count_mismatch_reject() {
    assert_code(
        "fn main() {\\n    a, b, c := 1, 2\\n    _ = a\\n    _ = b\\n    _ = c\\n}\\n",
        "HYB-E1002",
    );
    assert_code(
        "fn main() {\\n    d, e := 1, 2, 3\\n    _ = d\\n    _ = e\\n}\\n",
        "HYB-E1002",
    );
    assert_code(
        "fn main() {\\n    x, y := 1\\n    _ = x\\n    _ = y\\n}\\n",
        "HYB-E1002",
    );
}

#[test]
fn parallel_assign_count_mismatch_reject() {
    assert_code(
        "fn main() {\\n    var x int\\n    var y int\\n    x, y = 10, 20, 30\\n    _ = x\\n    _ = y\\n}\\n",
        "HYB-E1002",
    );
    assert_code(
        "fn main() {\\n    var x int\\n    var y int\\n    x, y = 10\\n    _ = x\\n    _ = y\\n}\\n",
        "HYB-E1002",
    );
}

#[test]
fn typed_decl_initializer_conflict_reject() {
    assert_code(
        "fn main() {\\n    var a int = \\"hello\\"\\n    _ = a\\n}\\n",
        "HYB-E1002",
    );
    assert_code(
        "fn main() {\\n    let b bool = 123\\n    _ = b\\n}\\n",
        "HYB-E1002",
    );
    assert_code(
        "fn main() {\\n    const s string = 456\\n    _ = s\\n}\\n",
        "HYB-E1002",
    );
    // NOTE: the grammar has no multi-name typed declaration (`var a, b int =
    // 1, 2` is E0002 at the parser), so the positional zip guard in `local`
    // is defensive class-shape; the single-name forms above are the issue's
    // actual repros.
}

#[test]
fn parallel_assign_positional_conflict_reject() {
    assert_code(
        "fn main() {\\n    x := 1\\n    s := \\"a\\"\\n    x, s = s, x\\n    _ = x\\n    _ = s\\n}\\n",
        "HYB-E1002",
    );
}

#[test]
fn valid_parallel_forms_stay_legal() {
    // matching counts stay legal
    assert_ok(
        "fn main() {\\n    a, b, c := 1, 2, 3\\n    _ = a\\n    _ = b\\n    _ = c\\n}\\n",
    );
    // same-type swap stays legal
    assert_ok(
        "fn main() {\\n    x := 1\\n    s := \\"a\\"\\n    x, s = x, s\\n    _ = x\\n    _ = s\\n}\\n",
    );
    // the untyped-int-to-float adaptation stays legal (kinds_conflict arm)
    assert_ok(
        "fn main() {\\n    var f float64 = 1\\n    _ = f\\n}\\n",
    );
    // an interface target accepts any initializer
    assert_ok(
        "fn main() {\\n    var a any = 1\\n    var b any = \\"str\\"\\n    _ = a\\n    _ = b\\n}\\n",
    );
    // matching counts in a parallel declaration stay legal
    assert_ok(
        "fn main() {\\n    a, b := 1, 2\\n    _ = a\\n    _ = b\\n}\\n",
    );
    // typed parallel assignment with matching kinds (the zip loop's happy path)
    assert_ok(
        "fn main() {\\n    var x int\\n    var y int\\n    x, y = 1, 2\\n    _ = x\\n    _ = y\\n}\\n",
    );
    // `f()?` on a (T1, T2, error) call yields two values after the unwrap —
    // stays legal (the 10-a5-http-handler-capture corpus class); the Try
    // source is fail-open for the count guards.
    assert_ok(
        "fn main() {\\n    code, body := http.get(\\"/x\\")?\\n    fmt.println(code, body)\\n}\\n",
    );
}
'''
)


def candidates_for(issue_no: int) -> list[dict]:
    """Round r = candidate r-1 of the issue's class variant (deterministic)."""
    variant = 1 if issue_no in (905, 903) else 0
    if variant == 0:
        return [
            {
                "name": "tuple-guards",
                "edits": _tuple_edits(),
                "new_files": [(TUPLE_TEST_PATH, TUPLE_TEST_SRC)],
                "rust_tests": ["--test", TUPLE_TEST_STEM],
                "rationale": (
                    "T-EBIND tuple-guards: the T23 binding-count rules now "
                    "cover non-call sources — a receive and a map index yield "
                    "at most two values (`v, ok`), an index of a known "
                    "slice/array/string base yields exactly one, a select "
                    "receive arm with 3+ names is rejected before the "
                    "emitter's unsupported lowering, and a 1-result call "
                    "bound to 2+ names is a plain assignment mismatch "
                    "(HYB-E1002). Class-level transplant of the #926 "
                    "count-guard shape onto every binding site; valid "
                    "1-value and comma-ok 2-value forms stay legal "
                    "(accept controls pinned)."
                ),
            },
        ]
    return [
        {
            "name": "buildable-guards",
            "edits": _buildable_edits(),
            "new_files": [(PAR_TEST_PATH, PAR_TEST_SRC)],
            "rust_tests": ["--test", PAR_TEST_STEM],
            "rationale": (
                "T-EBIND buildable-guards: parallel declarations/assignments "
                "gain the missing count guards (assignment mismatch), a "
                "provably single value bound to several names is rejected, "
                "typed declarations validate their initializers via "
                "kinds_conflict, and parallel assignments check every "
                "positional pair with the same conservative fail-open and "
                "the O8 slot-loan shape exemption the single-target form "
                "already pins. Class-level transplants of the #926 "
                "count-guard and #909/#969 kind-compat shapes; the untyped "
                "int-to-float adaptation and interface targets stay legal "
                "(accept controls pinned)."
            ),
        },
    ]


def candidates() -> list[dict]:
    """Default descent (brain API compatibility): the full union order."""
    return candidates_for(929) + candidates_for(905)


def _tuple_edits():
    edits = []

    # (0) import the kind model — the source-count classifier needs it.
    edits.append((TUPLE,
        """use crate::foreign;
use crate::model::{unwrap_parens, Checker, FnState, TypeShape};""",
        """use crate::buildable::OpKind;
use crate::foreign;
use crate::model::{unwrap_parens, Checker, FnState, TypeShape};"""))

    # (1) module-level: the source-count bound type.
    edits.append((TUPLE,
        """fn unwrap_parens_and_move(mut e: &Expr) -> &Expr {""",
        """/// Statically known value count of a non-call binding source (T23).
pub(crate) enum BindSourceBound {
    /// A channel receive / map index: `v` or `v, ok`.
    AtMost(usize),
    /// An index of a known slice/array/string base: exactly one value.
    Exact(usize),
}

fn unwrap_parens_and_move(mut e: &Expr) -> &Expr {"""))

    # (2) check_multi_value_bind: non-call source counts + 1-result mismatch.
    edits.append((TUPLE,
        """    fn check_multi_value_bind(&mut self, st: &FnState, value: &Expr, targets: usize, span: Span) {
        let Some((name, results)) = self.static_result_count(st, value) else {
            return;
        };
        if results < 2 || targets == results {
            return;
        }
        let msg = format!(
            "`{name}` returns {results} values, but {targets} {} bound: a multi-value result is not a tuple (T23)",
            if targets == 1 { "name is" } else { "names are" }
        );
        self.diags.push(
            Diagnostic::error("HYB-E2002", self.path.clone(), span, msg).with_notes(vec![
                "why: Hybrid has no tuple type, and Go rejects the emitted binding (`assignment mismatch`), so the program would not build".to_string(),
            ]),
        );
    }""",
        """    fn check_multi_value_bind(&mut self, st: &FnState, value: &Expr, targets: usize, span: Span) {
        let Some((name, results)) = self.static_result_count(st, value) else {
            // Non-call binding sources: a channel receive and a map index
            // yield at most two values (`v` or `v, ok`); indexing a slice,
            // array, or string yields exactly one and has no comma-ok form.
            // Binding a count the source cannot yield fails go build with
            // `assignment mismatch` — reject fail-closed as T23.
            if let Some(bound) = self.static_bind_source_bound(st, value) {
                let (desc, over) = match bound {
                    BindSourceBound::AtMost(n) => (
                        format!("yields at most {n} values (`v` or `v, ok`)"),
                        targets > n,
                    ),
                    BindSourceBound::Exact(n) => (
                        format!("yields exactly {n} value (no comma-ok form)"),
                        targets != n,
                    ),
                };
                if over {
                    let msg = format!(
                        "`{}` {desc}, but {targets} {} bound: the binding cannot yield that many values (T23)",
                        value.render(),
                        if targets == 1 { "name is" } else { "names are" }
                    );
                    self.diags.push(
                        Diagnostic::error("HYB-E2002", self.path.clone(), span, msg).with_notes(vec![
                            "why: Go rejects the emitted binding (`assignment mismatch`), so the program would not build".to_string(),
                            "fix: bind the count the source yields — `v` for an index, `v` or `v, ok` for a receive or map lookup".to_string(),
                        ]),
                    );
                }
            }
            return;
        };
        if results >= 2 {
            if targets == results {
                return;
            }
            let msg = format!(
                "`{name}` returns {results} values, but {targets} {} bound: a multi-value result is not a tuple (T23)",
                if targets == 1 { "name is" } else { "names are" }
            );
            self.diags.push(
                Diagnostic::error("HYB-E2002", self.path.clone(), span, msg).with_notes(vec![
                    "why: Hybrid has no tuple type, and Go rejects the emitted binding (`assignment mismatch`), so the program would not build".to_string(),
                ]),
            );
            return;
        }
        // results == 1: a 1-result source bound to 2+ names is a plain
        // assignment mismatch — Go refuses `assignment mismatch: N
        // variables but name returns 1 value`. Void sources (0 results)
        // stay owned by the void-position rule.
        if targets > 1 {
            let msg = format!(
                "`{name}` returns 1 value, but {targets} names are bound: assignment mismatch (T23 binding count)"
            );
            self.diags.push(
                Diagnostic::error("HYB-E1002", self.path.clone(), span, msg).with_notes(vec![
                    "why: Go rejects the emitted binding (`assignment mismatch`), so the program would not build".to_string(),
                    "fix: bind one name, or call a function that returns one value per target".to_string(),
                ]),
            );
        }
    }

    /// The statically known value count of a non-call binding source:
    /// a receive and a map index yield at most two values, an index of a
    /// known slice/array/string base yields exactly one. `None` fails
    /// open (unresolved bases, other shapes).
    fn static_bind_source_bound(&self, st: &FnState, expr: &Expr) -> Option<BindSourceBound> {
        match unwrap_parens_and_move(expr) {
            Expr::Receive { .. } => Some(BindSourceBound::AtMost(2)),
            Expr::Index { base, .. } => match self.shape_of_expr(st, base) {
                TypeShape::Map(..) => Some(BindSourceBound::AtMost(2)),
                TypeShape::Slice(..) | TypeShape::Array(..) | TypeShape::Str => {
                    Some(BindSourceBound::Exact(1))
                }
                // a pointer to an array auto-derefs on indexing in Go —
                // still exactly one value; every other pointer/base stays
                // fail-open.
                TypeShape::Pointer(inner) => match inner.as_ref() {
                    TypeShape::Array(..) => Some(BindSourceBound::Exact(1)),
                    _ => None,
                },
                _ => None,
            },
            _ => None,
        }
    }"""))

    # (3) select receive arms: 3+ names crash the emitter's select lowering.
    edits.append((TUPLE,
        """                        SelectArmKind::Recv { chan, .. } => {
                            self.check_expr_single_value(st, chan, chan.span());
                        }""",
        """                        SelectArmKind::Recv { names, chan } => {
                            self.check_expr_single_value(st, chan, chan.span());
                            // A receive yields at most two values (`v, ok`);
                            // 3+ names crash the emitter's select lowering —
                            // reject fail-closed as T23 before emission.
                            if names.len() > 2 {
                                let msg = format!(
                                    "select receive binds {} names, but a receive yields at most 2 values (`v` or `v, ok`) (T23)",
                                    names.len()
                                );
                                self.diags.push(
                                    Diagnostic::error("HYB-E2002", self.path.clone(), chan.span(), msg)
                                        .with_notes(vec![
                                            "why: the emitter refuses select receive with 3+ names, so the program would not build".to_string(),
                                            "fix: bind at most two names — `v := <-ch` or `v, ok := <-ch`".to_string(),
                                        ]),
                                );
                            }
                        }"""))

    # (4)+(5) visibility: tuple.rs classifies index bases through the kind
    # model (crate-internal use — no behavior change).
    edits.append((BUILDABLE,
        """#[derive(Debug, Clone, PartialEq, Eq)]
enum OpKind {""",
        """#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum OpKind {"""))
    edits.append((BUILDABLE,
        """    fn kind_of_expr(&self, e: &Expr) -> OpKind {""",
        """    pub(crate) fn kind_of_expr(&self, e: &Expr) -> OpKind {"""))

    return edits


def _buildable_edits():
    edits = []

    # (1) module-level: the provably-single-value classifier.
    edits.append((BUILDABLE,
        """/// Symmetrical conflict check for equality operators (`==`, `!=`).""",
        """/// Is `e` provably a single-value expression (the #905/#903 binding-count
/// class)?
///
/// Conservative: `true` only for shapes that cannot yield two values in
/// any context. Calls, receives, and index expressions are excluded —
/// their counts are owned by the source-count rules (tuple.rs T23 and the
/// #926 void-position rule) — and an unresolved shape fails open.
fn is_definitely_single_value(e: &Expr) -> bool {
    match unwrap_parens(e) {
        Expr::Lit { .. }
        | Expr::Ident { .. }
        | Expr::Binary { .. }
        | Expr::Unary { .. }
        | Expr::AddrOf { .. }
        | Expr::Field { .. }
        | Expr::Interp { .. }
        | Expr::Slice { .. }
        | Expr::StructLit { .. }
        | Expr::SliceLit { .. }
        | Expr::MapLit { .. }
        | Expr::FnLit { .. } => true,
        // NOTE: `Expr::Try` is deliberately NOT here — `f()?` on a
        // (T1, T2, error) call yields TWO values after the unwrap (the
        // accept-corpus program 10-a5-http-handler-capture pins
        // `code, body := httptest.get(...)?` as legal), so a Try is a
        // source-count question, not a provably-single value.
        Expr::Move { operand, .. } => is_definitely_single_value(operand),
        _ => false,
    }
}

/// Symmetrical conflict check for equality operators (`==`, `!=`)."""))

    # (2) local(): parallel-decl count guards + typed-decl initializer compat.
    edits.append((BUILDABLE,
        """        if l.names.len() == 1 && l.values.len() == 1 {
            self.check_single_value_position(&l.values[0], "a binding", /*binding_position=*/ true);
        }""",
        """        if l.names.len() == 1 && l.values.len() == 1 {
            self.check_single_value_position(&l.values[0], "a binding", /*binding_position=*/ true);
        }
        // Binding-count guards: a declaration whose value count cannot
        // match its names fails go build with `assignment mismatch`.
        if l.values.len() > 1 && l.names.len() != l.values.len() {
            self.emit_e1002(
                l.span,
                format!(
                    "assignment mismatch: {} variables but {} values (HYB-E1002)",
                    l.names.len(),
                    l.values.len()
                ),
                "a parallel declaration binds positionally; the name and value counts must match",
            );
        }
        if l.names.len() > 1
            && l.values.len() == 1
            && is_definitely_single_value(&l.values[0])
        {
            self.emit_e1002(
                l.span,
                format!(
                    "assignment mismatch: {} variables but the initializer yields exactly 1 (HYB-E1002)",
                    l.names.len()
                ),
                "call, receive, and index sources are checked by the source-count rules; a provably single value binds one name",
            );
        }
        // A typed declaration must accept its initializers — Go refuses
        // `cannot use ... as ... value in variable declaration`.
        if let Some(t) = &l.ty {
            let want = self.kind_of_ty(t, 0);
            if l.values.len() == 1 && l.names.len() == 1 {
                let vk = self.kind_of_expr(&l.values[0]);
                if kinds_conflict(&want, &vk) {
                    self.emit_e1002(
                        l.span,
                        format!(
                            "cannot initialize `{}` (type `{}`) with a value of type `{}` — Go refuses `cannot use ... as {} value in variable declaration` (HYB-E1002)",
                            l.names[0],
                            want.label(),
                            vk.label(),
                            want.label()
                        ),
                        "the declared type is fixed; write an explicit conversion when a conversion is intended",
                    );
                }
            } else if l.values.len() > 1 && l.values.len() == l.names.len() {
                for (name, v) in l.names.iter().zip(l.values.iter()) {
                    let vk = self.kind_of_expr(v);
                    if kinds_conflict(&want, &vk) {
                        self.emit_e1002(
                            l.span,
                            format!(
                                "cannot initialize `{}` (type `{}`) with a value of type `{}` — Go refuses `cannot use ... as {} value in variable declaration` (HYB-E1002)",
                                name,
                                want.label(),
                                vk.label(),
                                want.label()
                            ),
                            "the declared type is fixed; write an explicit conversion when a conversion is intended",
                        );
                    }
                }
            }
        }"""))

    # (3) check_assign_operands: parallel-assign counts + positional kinds.
    edits.append((BUILDABLE,
        """            AssignOp::Assign => {
                if targets.len() == 1 && values.len() == 1 {
                    // O8 slot-loan (adapted during integration): `q = &m["a"]`
                    // on a value-typed `q` does NOT emit `q = &m["a"]` — the
                    // emitter lowers the loan to `q = m["a"]` plus a store-back
                    // into the slot, so a `Struct` target receiving a
                    // `Pointer` value here is legal Hybrid, not a mismatch
                    // (map_element_addressable's
                    // accepts_slot_loan_into_value_var pins the shape).
                    // Fail open for exactly that shape: an addr-of whose
                    // operand is an index or field place. Loans are validated
                    // for addressability by addressable.rs (HYB-E2010), so
                    // the non-addressable ones still reject.
                    let is_loan_shape = as_addr_of(&values[0])
                        .is_some_and(|o| matches!(o, Expr::Index { .. } | Expr::Field { .. }));
                    let vk = self.kind_of_expr(&values[0]);
                    if !is_loan_shape && kinds_conflict(&tk, &vk) {
                        self.emit_e1002(
                            span,
                            format!(
                                "cannot assign a value of type `{}` to `{}` (type `{}`) — Go refuses `cannot use ... as {} value in assignment` (HYB-E1002)",
                                vk.label(),
                                target.render(),
                                tk.label(),
                                tk.label()
                            ),
                            "the target's declared type is fixed; write an explicit conversion when a conversion is intended",
                        );
                    }
                }
            }""",
        """            AssignOp::Assign => {
                if targets.len() == 1 && values.len() == 1 {
                    // O8 slot-loan (adapted during integration): `q = &m["a"]`
                    // on a value-typed `q` does NOT emit `q = &m["a"]` — the
                    // emitter lowers the loan to `q = m["a"]` plus a store-back
                    // into the slot, so a `Struct` target receiving a
                    // `Pointer` value here is legal Hybrid, not a mismatch
                    // (map_element_addressable's
                    // accepts_slot_loan_into_value_var pins the shape).
                    // Fail open for exactly that shape: an addr-of whose
                    // operand is an index or field place. Loans are validated
                    // for addressability by addressable.rs (HYB-E2010), so
                    // the non-addressable ones still reject.
                    let is_loan_shape = as_addr_of(&values[0])
                        .is_some_and(|o| matches!(o, Expr::Index { .. } | Expr::Field { .. }));
                    let vk = self.kind_of_expr(&values[0]);
                    if !is_loan_shape && kinds_conflict(&tk, &vk) {
                        self.emit_e1002(
                            span,
                            format!(
                                "cannot assign a value of type `{}` to `{}` (type `{}`) — Go refuses `cannot use ... as {} value in assignment` (HYB-E1002)",
                                vk.label(),
                                target.render(),
                                tk.label(),
                                tk.label()
                            ),
                            "the target's declared type is fixed; write an explicit conversion when a conversion is intended",
                        );
                    }
                } else if values.len() > 1 && values.len() != targets.len() {
                    // Binding-count guards: a parallel assignment is
                    // positional — a count mismatch fails go build with
                    // `assignment mismatch`.
                    self.emit_e1002(
                        span,
                        format!(
                            "assignment mismatch: {} variables but {} values (HYB-E1002)",
                            targets.len(),
                            values.len()
                        ),
                        "a parallel assignment binds positionally; the target and value counts must match",
                    );
                } else if targets.len() > 1 && values.len() == targets.len() {
                    // Every positional pair must satisfy the same kind check
                    // as the single-target form, with the same O8 loan-shape
                    // fail-open.
                    for (t, v) in targets.iter().zip(values.iter()) {
                        if as_addr_of(v)
                            .is_some_and(|o| matches!(o, Expr::Index { .. } | Expr::Field { .. }))
                        {
                            continue;
                        }
                        let tpk = self.kind_of_place(t);
                        let vk = self.kind_of_expr(v);
                        if kinds_conflict(&tpk, &vk) {
                            self.emit_e1002(
                                v.span(),
                                format!(
                                    "cannot assign a value of type `{}` to `{}` (type `{}`) in parallel assignment — Go refuses `cannot use ... as {} value in assignment` (HYB-E1002)",
                                    vk.label(),
                                    t.render(),
                                    tpk.label(),
                                    tpk.label()
                                ),
                                "each target's declared type is fixed; write an explicit conversion when a conversion is intended",
                            );
                        }
                    }
                } else if targets.len() > 1 && values.len() == 1 {
                    // One provably-single value bound to several targets —
                    // go build refuses with `assignment mismatch`. Call,
                    // receive, and index sources are owned by the
                    // source-count rules.
                    if is_definitely_single_value(&values[0]) {
                        self.emit_e1002(
                            span,
                            format!(
                                "assignment mismatch: {} variables but the value yields exactly 1 (HYB-E1002)",
                                targets.len()
                            ),
                            "bind one name per value, or use a source that yields one value per target",
                        );
                    }
                }
            }"""))

    return edits
