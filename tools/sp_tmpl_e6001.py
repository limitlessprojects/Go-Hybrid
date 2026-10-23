#!/usr/bin/env python3
"""sp_tmpl_e6001.py — T-E6001 template family (bake-off arm A0, issue #758).

CLASS-LEVEL transplants only (AUTHOR-STAGE.md §5 amendment 1):
  * #998 pattern — resolve constant expressions through the file const map
    (`const_int_fold_with` / `const_float_fold_with`), shadow-checked against
    the live FnState; a bare syntactic fold sees a named const as NotConst.
  * #725 pattern — constant conversions fold (the Call arm already in
    `const_int_fold_with`); the float side gets its mirror leaf.
  * #999 pattern — mirror an existing predicate onto an uncovered site
    (the array-length type-expression walk calls the SAME zero-divisor
    predicate).
  * Go semantics: `len()` of a constant string is a compile-time constant.

Completion contract = every family the issue enumerates:
  1. named const int/float divisors (file-level; shadowed locals stay legal)
  2. folded float zero divisors + float/int conversion divisors
  3. `len("<const>")` leaves in divisors (incl. named const strings)
  4. array-length `[1/0]int` across declaration sites
Block-scoped consts stay fail-open (FnState carries no const-value map —
the #998 precedent), disclosed in the rationale.
"""

from __future__ import annotations

ARITH = "compiler/crates/hy-check/src/arith.rs"
EMIT = "compiler/crates/hy-check/src/emit_guard.rs"
MODEL = "compiler/crates/hy-check/src/model.rs"
LIB = "compiler/crates/hy-check/src/lib.rs"

# --- the new helper code appended to arith.rs -------------------------------

HELPERS = '''
// ---------------------------------------------------------------------------
// Issue #758: named/derived constant zero divisors (HYB-E6001).
// The #998 pattern: resolve through the file const map exactly like the
// declaration path; a bare syntactic fold sees a named const as NotConst.
// ---------------------------------------------------------------------------

/// Conservative fail-open guard: every name DEFINED anywhere inside a
/// deep-walked closure body. Closure bodies walk under the caller's scope
/// state (`check_stmt_const_zero_divisor_inner` deep mode), so a
/// closure-local shadow is invisible to `st`. Over-collecting is safe —
/// it only suppresses resolution (a missed detection, never a false
/// reject); the house rule is fail-open.
fn expr_defined_names(expr: &Expr, out: &mut std::collections::HashSet<String>) {
    match expr {
        Expr::FnLit { params, body, .. } => {
            for p in params {
                out.insert(p.name.clone());
            }
            for s in &body.stmts {
                stmt_defined_names(s, out);
            }
        }
        Expr::Unary { operand, .. }
        | Expr::AddrOf { operand, .. }
        | Expr::Move { operand, .. }
        | Expr::Try { operand, .. }
        | Expr::Paren { inner: operand, .. }
        | Expr::Field { base: operand, .. }
        | Expr::Receive { chan: operand, .. } => expr_defined_names(operand, out),
        Expr::Index { base, index, .. } => {
            expr_defined_names(base, out);
            expr_defined_names(index, out);
        }
        Expr::Slice { base, lo, hi, .. } => {
            expr_defined_names(base, out);
            if let Some(lo) = lo {
                expr_defined_names(lo, out);
            }
            if let Some(hi) = hi {
                expr_defined_names(hi, out);
            }
        }
        Expr::Call { callee, args, .. } => {
            expr_defined_names(callee, out);
            for a in args {
                expr_defined_names(&a.value, out);
            }
        }
        Expr::StructLit { fields, .. } => {
            for f in fields {
                expr_defined_names(&f.value, out);
            }
        }
        Expr::SliceLit { items, .. } => {
            for i in items {
                expr_defined_names(i, out);
            }
        }
        Expr::MapLit { entries, .. } => {
            for (k, v) in entries {
                expr_defined_names(k, out);
                expr_defined_names(v, out);
            }
        }
        Expr::Interp { parts, .. } => {
            for p in parts {
                if let InterpPart::Expr(e) = p {
                    expr_defined_names(e, out);
                }
            }
        }
        _ => {}
    }
}

fn stmt_defined_names(stmt: &Stmt, out: &mut std::collections::HashSet<String>) {
    match stmt {
        Stmt::Local(l) => {
            for n in &l.names {
                out.insert(n.clone());
            }
            for v in &l.values {
                expr_defined_names(v, out);
            }
        }
        Stmt::Assign { targets, values, .. } => {
            for t in targets {
                if let PlaceKind::Ident(n) = &t.kind {
                    out.insert(n.clone());
                }
            }
            for v in values {
                expr_defined_names(v, out);
            }
        }
        Stmt::For { kind, body, .. } => {
            match kind {
                ForKind::Range { var, lo, hi, .. } => {
                    out.insert(var.clone());
                    expr_defined_names(lo, out);
                    expr_defined_names(hi, out);
                }
                ForKind::Iterate { vars, expr, .. } => {
                    for v in vars {
                        out.insert(v.clone());
                    }
                    expr_defined_names(expr, out);
                }
                ForKind::While { cond } => expr_defined_names(cond, out),
                ForKind::Infinite => {}
            }
            for s in &body.stmts {
                stmt_defined_names(s, out);
            }
        }
        Stmt::Match { subject, arms, .. } => {
            expr_defined_names(subject, out);
            for arm in arms {
                if let Some(g) = &arm.guard {
                    expr_defined_names(g, out);
                }
                for s in &arm.body.stmts {
                    stmt_defined_names(s, out);
                }
            }
        }
        Stmt::If {
            init, cond, then, otherwise, ..
        } => {
            if let Some(init) = init {
                stmt_defined_names(init, out);
            }
            expr_defined_names(cond, out);
            for s in &then.stmts {
                stmt_defined_names(s, out);
            }
            if let Some(other) = otherwise {
                stmt_defined_names(other, out);
            }
        }
        Stmt::Block { block, .. } => {
            for s in &block.stmts {
                stmt_defined_names(s, out);
            }
        }
        Stmt::Managed { body, .. } => {
            for s in &body.stmts {
                stmt_defined_names(s, out);
            }
        }
        Stmt::Select { arms, .. } => {
            for arm in arms {
                match &arm.kind {
                    SelectArmKind::Recv { names, chan, .. } => {
                        for n in names {
                            out.insert(n.clone());
                        }
                        expr_defined_names(chan, out);
                    }
                    SelectArmKind::Send { chan, value, .. } => {
                        expr_defined_names(chan, out);
                        expr_defined_names(value, out);
                    }
                    SelectArmKind::Default => {}
                }
                for s in &arm.body.stmts {
                    stmt_defined_names(s, out);
                }
            }
        }
        Stmt::Send { chan, value, .. } => {
            expr_defined_names(chan, out);
            expr_defined_names(value, out);
        }
        Stmt::Expr { expr, .. } => expr_defined_names(expr, out),
        Stmt::Return { values, .. } => {
            for v in values {
                expr_defined_names(v, out);
            }
        }
        Stmt::Task { callee, args, .. } => {
            expr_defined_names(callee, out);
            for a in args {
                expr_defined_names(&a.value, out);
            }
        }
        Stmt::Defer { call, .. } => expr_defined_names(call, out),
        Stmt::Break { .. } | Stmt::Continue { .. } => {}
    }
}

/// `len("abc")` — Go evaluates the length of a constant string at compile
/// time (spec: constant expression). Named const strings resolve through
/// the same map. The length is the BYTE length (Go `len` on a string).
fn const_len_str_value(
    e: &Expr,
    consts: &std::collections::HashMap<String, &Expr>,
    defined: &std::collections::HashSet<String>,
    depth: u32,
) -> Option<i128> {
    if depth > 16 {
        return None;
    }
    if let Expr::Call { callee, args, .. } = unwrap_parens(e) {
        if let Expr::Ident { name, .. } = unwrap_parens(callee) {
            if name == "len" && args.len() == 1 && !args[0].spread {
                let a = unwrap_parens(&args[0].value);
                let s: Option<&String> = match a {
                    Expr::Lit {
                        kind: LitKind::Str(s) | LitKind::RawStr(s),
                        ..
                    } => Some(s),
                    Expr::Ident { name: n, .. } if !defined.contains(n) => {
                        consts.get(n.as_str()).and_then(|v| match unwrap_parens(v) {
                            Expr::Lit {
                                kind: LitKind::Str(s) | LitKind::RawStr(s),
                                ..
                            } => Some(s),
                            _ => None,
                        })
                    }
                    _ => None,
                };
                if let Some(s) = s {
                    return Some(s.len() as i128);
                }
            }
        }
    }
    None
}

/// The #758 divisor fold: literals, named consts (shadow-checked against
/// the live env AND the deep-walk tree's defines), binary arithmetic over
/// folded leaves, and `len()` of constant strings. Everything else —
/// including fixed-width int conversions of constants — delegates to the
/// shared `const_int_fold_with` (#725 machinery).
fn divisor_int_fold(
    e: &Expr,
    consts: &std::collections::HashMap<String, &Expr>,
    st: &crate::model::FnState,
    defined: &std::collections::HashSet<String>,
    depth: u32,
) -> crate::const_overflow::ConstFold {
    use crate::const_overflow::{const_int_fold_with, ConstFold};
    if depth > 16 {
        return ConstFold::NotConst;
    }
    let bare = unwrap_parens(e);
    if let Some(n) = const_len_str_value(bare, consts, defined, depth) {
        return ConstFold::Val(n);
    }
    match bare {
        Expr::Ident { name, .. } => {
            if st.env_lookup(name).is_some() || defined.contains(name) {
                return ConstFold::NotConst;
            }
            if let Some(v) = consts.get(name.as_str()) {
                return divisor_int_fold(v, consts, st, defined, depth + 1);
            }
            ConstFold::NotConst
        }
        Expr::Binary { op, lhs, rhs, .. } => {
            let l = divisor_int_fold(lhs, consts, st, defined, depth + 1);
            let r = divisor_int_fold(rhs, consts, st, defined, depth + 1);
            crate::const_overflow::fold_bin(*op, l, r)
        }
        _ => const_int_fold_with(e, consts, depth, Some(st)),
    }
}

fn divisor_int_fold_is_zero(
    e: &Expr,
    consts: &std::collections::HashMap<String, &Expr>,
    st: &crate::model::FnState,
    defined: &std::collections::HashSet<String>,
) -> bool {
    matches!(
        divisor_int_fold(e, consts, st, defined, 0),
        crate::const_overflow::ConstFold::Val(0)
    )
}

/// Float-conversion zero leaves: `float64(0)`, `f64(1 - 1)` — the float
/// mirror of the int-conversion arm in `const_int_fold_with` (#725).
fn divisor_float_conv_zero(
    e: &Expr,
    consts: &std::collections::HashMap<String, &Expr>,
    st: &crate::model::FnState,
    defined: &std::collections::HashSet<String>,
    depth: u32,
) -> bool {
    if depth > 16 {
        return false;
    }
    if let Expr::Call { callee, args, .. } = unwrap_parens(e) {
        if args.len() == 1 && !args[0].spread {
            if let Expr::Ident { name, .. } = unwrap_parens(callee) {
                if matches!(name.as_str(), "f32" | "f64" | "float32" | "float64") {
                    let a = &args[0].value;
                    if divisor_int_fold_is_zero(a, consts, st, defined) {
                        return true;
                    }
                    if matches!(
                        crate::const_overflow::const_float_fold_with(a, consts, 0, Some(st)),
                        Some(f) if f == 0.0
                    ) {
                        return true;
                    }
                }
            }
        }
    }
    false
}

fn divisor_float_fold_is_zero(
    e: &Expr,
    consts: &std::collections::HashMap<String, &Expr>,
    st: &crate::model::FnState,
    defined: &std::collections::HashSet<String>,
) -> bool {
    if divisor_float_conv_zero(e, consts, st, defined, 0) {
        return true;
    }
    matches!(
        crate::const_overflow::const_float_fold_with(e, consts, 0, Some(st)),
        Some(f) if f == 0.0
    )
}

/// Const-ness of the float-branch dividend, map-aware: the syntactic rule
/// (`is_const_expr`) plus anything the int/float folds can prove.
fn is_const_map_expr(
    e: &Expr,
    consts: &std::collections::HashMap<String, &Expr>,
    st: &crate::model::FnState,
    defined: &std::collections::HashSet<String>,
) -> bool {
    if is_const_expr(e) {
        return true;
    }
    !matches!(
        divisor_int_fold(e, consts, st, defined, 0),
        crate::const_overflow::ConstFold::NotConst
    ) || crate::const_overflow::const_float_fold_with(e, consts, 0, Some(st)).is_some()
}
'''

# --- the emitted test file --------------------------------------------------

TEST_PATH = "compiler/crates/hy-check/tests/const_zero_divisor_const_forms.rs"
TEST_STEM = "const_zero_divisor_const_forms"

TEST_SRC = '''//! #758 — named/derived constant zero divisors (HYB-E6001).
//!
//! Go folds constant expressions BEFORE deciding `invalid operation:
//! division by zero`, so `10 / zero` (`const zero = 0`) and
//! `1.0 / (1.0 - 1.0)` are compile errors. `hyb check` accepted them with
//! 0 diagnostics because `const_eval_int` / `is_float_literal_zero` only
//! inspected inline literals. The fix makes the divisor predicate
//! const-map-aware (#998 pattern) and mirrors the SAME predicate onto the
//! array-length site (#999 pattern). Runtime divisors and shadowed
//! bindings stay legal (fail-open outside the constant world).

use hy_check::check_file;
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

#[test]
fn named_const_int_zero_divisor_rejects() {
    assert_code(
        "const zero = 0\\n\\nfn main() {\\n    x := 10 / zero\\n    _ = x\\n}\\n",
        "HYB-E6001",
    );
    assert_code(
        "const zero = 0\\n\\nfn main() {\\n    x := 10 % zero\\n    _ = x\\n}\\n",
        "HYB-E6001",
    );
    assert_code(
        "const a = 3\\nconst b = a - 3\\n\\nfn main() {\\n    x := 10\\n    x /= b\\n    _ = x\\n}\\n",
        "HYB-E6001",
    );
}

#[test]
fn folded_float_zero_divisor_rejects() {
    assert_code(
        "fn main() {\\n    _ = 1.0 / (1.0 - 1.0)\\n}\\n",
        "HYB-E6001",
    );
    assert_code(
        "fn main() {\\n    _ = 1.0 / (0.0 * 5.0)\\n}\\n",
        "HYB-E6001",
    );
}

#[test]
fn named_const_float_zero_divisor_rejects() {
    assert_code(
        "const f_val = 1.0\\nconst fz = 0.0\\n\\nfn main() {\\n    _ = f_val / fz\\n}\\n",
        "HYB-E6001",
    );
}

#[test]
fn conversion_zero_divisor_rejects() {
    assert_code(
        "fn main() {\\n    x := 10 / int(0)\\n    _ = x\\n}\\n",
        "HYB-E6001",
    );
    assert_code(
        "fn main() {\\n    x := 10 % int64(1 - 1)\\n    _ = x\\n}\\n",
        "HYB-E6001",
    );
    assert_code(
        "fn main() {\\n    _ = 1.0 / float64(0)\\n}\\n",
        "HYB-E6001",
    );
    assert_code(
        "fn main() {\\n    _ = 1.0 / f64(1 - 1)\\n}\\n",
        "HYB-E6001",
    );
}

#[test]
fn const_string_len_zero_divisor_rejects() {
    assert_code(
        "fn main() {\\n    y := 10 / (len(\\"abc\\") - 3)\\n    _ = y\\n}\\n",
        "HYB-E6001",
    );
    assert_code(
        "const s_val = \\"abc\\"\\n\\nfn main() {\\n    x := 10 % (len(s_val) - 3)\\n    _ = x\\n}\\n",
        "HYB-E6001",
    );
}

#[test]
fn array_len_zero_divisor_rejects() {
    assert_code(
        "fn main() {\\n    var a [1 / 0]int\\n    _ = a\\n}\\n",
        "HYB-E6001",
    );
    assert_code(
        "type arr = [1 / 0]int\\n\\nfn main() {\\n    _ = 1\\n}\\n",
        "HYB-E6001",
    );
    assert_code(
        "struct s { a [1 % 0]int }\\n\\nfn main() {\\n    _ = 1\\n}\\n",
        "HYB-E6001",
    );
    assert_code(
        "fn f(a [1 / 0]int) {\\n    _ = a\\n}\\n\\nfn main() {\\n    _ = 1\\n}\\n",
        "HYB-E6001",
    );
    assert_code(
        "fn main() {\\n    _ = [1 / 0]int{}\\n}\\n",
        "HYB-E6001",
    );
}

#[test]
fn runtime_and_shadowed_divisors_stay_legal() {
    // runtime divisor: panics at run time, Go accepts the program
    assert_ok("fn main() {\\n    var n int = 0\\n    x := 10 / n\\n    _ = x\\n}\\n");
    // a local shadowing the file const is a RUNTIME binding (issue control)
    assert_ok(
        "const zero = 0\\n\\nfn main() {\\n    zero := 2\\n    x := 10 / zero\\n    _ = x\\n}\\n",
    );
    // non-zero constants divide fine
    assert_ok("const k = 5\\n\\nfn main() {\\n    x := 10 / k\\n    _ = x\\n}\\n");
    // in-range integer division of constants
    assert_ok("fn main() {\\n    x := 10 / 2\\n    _ = x\\n}\\n");
    // len of a runtime string is not a constant
    assert_ok("fn main() {\\n    s := \\"abc\\"\\n    x := 10 / (len(s) - 3)\\n    _ = x\\n}\\n");
    // legal array lengths, incl. a zero length
    assert_ok("fn main() {\\n    var a [0]int\\n    var b [4 / 2]int\\n    _ = a\\n    _ = b\\n}\\n");
}
'''

FIXTURE_PATH = "tests/checker-corpus/const-zero-divisor-const-forms-reject.hy"
FIXTURE_SRC = '''// ============================================================================
// const-zero-divisor-const-forms-reject — Checker corpus (issue #758).
//
// PURPOSE: zero divisors in DERIVED constant forms — a named file-level
// `const`, a folded float expression, `len()` of a constant string, and a
// zero divisor inside an array-length expression — not just the literal
// `0` twin the #149 rule already caught.
//
// RULE CITATION: GUARANTEES.md Row 13 (BUILDABLE) — every `hyb check`-
// accepted file must build cleanly with `go build`. Go folds constant
// expressions first and refuses all of these at compile time:
//   invalid operation: division by zero
//
// ---- Why this is a REJECT ----
// The divisor predicate only inspected inline literals (`is_int_literal_zero`,
// `is_float_literal_zero`), so named consts, folded float expressions,
// constant conversions, constant-string `len()`, and array-length positions
// passed with 0 diagnostics. The fix makes the predicate const-map-aware
// (#998 pattern: resolve through the file const map, shadow-checked) and
// mirrors it onto the array-length site (#999 pattern: same predicate,
// uncovered position). A runtime divisor (`10 / n`) still panics at run
// time and stays accepted; a local shadowing the const name is a runtime
// binding and stays accepted too.
//
// machine-readable verdict marker below; regression pins live in
// `hy-check/tests/const_zero_divisor_const_forms.rs`.
// ============================================================================

const zero = 0
const s_val = "abc"

fn main() {
    x := 10 / zero
    _ = 1.0 / (1.0 - 1.0)
    y := 10 / (len(s_val) - 3)
    var a [1 / 0]int
    _ = x
    _ = y
    _ = a
}

// EXPECTED-VERDICT: REJECT BUILDABLE
'''


def candidates():
    """Deterministic descent: full family → narrower variants."""
    c_full = {
        "name": "full",
        "edits": _edits(with_len_leaves=True, with_float_conv=True),
        "new_files": [
            (TEST_PATH, TEST_SRC),
        ],
        "rust_tests": ["--test", TEST_STEM],
        "rationale": (
            "T-E6001 full: divisor predicate becomes const-map-aware "
            "(#998 pattern) with len()-of-const-string leaves and the "
            "float-conversion leaf; the SAME predicate is mirrored onto "
            "check_array_len (#999 pattern). Block-scoped consts stay "
            "fail-open (#998 precedent). Pre-verified internally: build, "
            "repro REJECT+code, accept controls, emitted tests, corpus, census."
        ),
    }
    c_core = {
        "name": "core",
        "edits": _edits(with_len_leaves=False, with_float_conv=False),
        "new_files": [
            (TEST_PATH, TEST_SRC_CORE_FALLBACK),
        ],
        "rust_tests": ["--test", TEST_STEM],
        "rationale": (
            "T-E6001 core (descent): named-const int/float divisors only — "
            "len() leaves and the float-conversion leaf dropped after gate "
            "evidence. Array-length mirror retained."
        ),
    }
    return [c_full, c_core]


def _drop_fns(src: str, *names: str) -> str:
    """Remove whole #[test] functions (core variant cannot support them)."""
    for name in names:
        marker = f"#[test]\nfn {name}() {{"
        i = src.find(marker)
        if i < 0:
            continue
        j = src.find("\n}\n", i)
        src = src[:i] + src[j + len("\n}\n"):]
    return src


TEST_SRC_CORE_FALLBACK = _drop_fns(
    TEST_SRC,
    "const_string_len_zero_divisor_rejects",
    "conversion_zero_divisor_rejects",
)


def _edits(with_len_leaves: bool, with_float_conv: bool):
    """The exact single-occurrence anchor edits for T-E6001."""
    edits = []

    # (0) Checker field: the deep-walk defined-names guard (fail-open).
    edits.append((MODEL,
        "    pub fns: HashMap<String, &'a FnDecl>,",
        "    pub fns: HashMap<String, &'a FnDecl>,\n"
        "    /// Issue #758: names defined inside the deep-walked closure body\n"
        "    /// currently under the zero-divisor walk — a closure-local shadow\n"
        "    /// is invisible to `st`, so resolution fails open on them.\n"
        "    pub zd_defined: std::collections::HashSet<String>,"))
    edits.append((MODEL,
        "            fns: HashMap::new(),",
        "            fns: HashMap::new(),\n            zd_defined: std::collections::HashSet::new(),"))

    # (1) arith.rs: thread `st` through the walker family (replace-all on
    # call sites, then signatures).
    edits.append((ARITH,
        "self.check_expr_const_zero_divisor(",
        "self.check_expr_const_zero_divisor(st, ",
        True))
    edits.append((ARITH,
        "pub(crate) fn check_expr_const_zero_divisor(&mut self, expr: &Expr) {",
        "pub(crate) fn check_expr_const_zero_divisor(\n        &mut self,\n        st: &crate::model::FnState,\n        expr: &Expr,\n    ) {"))
    edits.append((ARITH,
        "self.check_place_const_zero_divisor(",
        "self.check_place_const_zero_divisor(st, ",
        True))
    edits.append((ARITH,
        "fn check_place_const_zero_divisor(&mut self, place: &Place) {",
        "fn check_place_const_zero_divisor(&mut self, st: &crate::model::FnState, place: &Place) {"))
    edits.append((ARITH,
        "self.check_stmt_const_zero_divisor_inner(",
        "self.check_stmt_const_zero_divisor_inner(st, ",
        True))
    edits.append((ARITH,
        "fn check_stmt_const_zero_divisor_inner(&mut self, stmt: &Stmt, deep: bool) {",
        "fn check_stmt_const_zero_divisor_inner(\n        &mut self,\n        st: &crate::model::FnState,\n        stmt: &Stmt,\n        deep: bool,\n    ) {"))
    edits.append((ARITH,
        "pub(crate) fn check_stmt_const_zero_divisor(&mut self, stmt: &Stmt) {\n        self.check_file_close_and_range_directions(stmt);\n        self.check_stmt_const_zero_divisor_inner(st, stmt, /*deep=*/ false);",
        "pub(crate) fn check_stmt_const_zero_divisor(&mut self, st: &crate::model::FnState, stmt: &Stmt) {\n        self.check_file_close_and_range_directions(stmt);\n        self.zd_defined.clear();\n        self.check_stmt_const_zero_divisor_inner(st, stmt, /*deep=*/ false);"))
    # file-const pass: no live env, no deep tree — empty state
    edits.append((ARITH,
        "self.check_expr_const_zero_divisor(st, &c.value);",
        "self.check_expr_const_zero_divisor(&crate::model::FnState::default(), &c.value);"))
    # lib.rs caller: thread the live FnState
    edits.append((LIB,
        "ck.check_stmt_const_zero_divisor(stmt);",
        "ck.check_stmt_const_zero_divisor(&st, stmt);"))

    # (2) the FnLit deep-walk arm: closure bodies walk under the caller's
    # scope state — collect the body's defines into the fail-open guard.
    edits.append((ARITH,
        "            Expr::FnLit { body, .. } => {\n                // Closure bodies are not flattened into Pass B — deep-walk.\n                for stmt in &body.stmts {\n                    self.check_stmt_const_zero_divisor_inner(st, stmt, /*deep=*/ true);\n                }\n            }",
        "            Expr::FnLit { params, body, .. } => {\n                // Closure bodies are not flattened into Pass B — deep-walk.\n                // Their scopes differ from `st`: a closure-local shadow of a\n                // file const is invisible to env_lookup, so resolution fails\n                // open on every name the body defines (issue #758 guard).\n                {\n                    let mut defined = std::collections::HashSet::new();\n                    for p in params {\n                        defined.insert(p.name.clone());\n                    }\n                    for stmt in &body.stmts {\n                        stmt_defined_names(stmt, &mut defined);\n                    }\n                    self.zd_defined = defined;\n                }\n                for stmt in &body.stmts {\n                    self.check_stmt_const_zero_divisor_inner(st, stmt, /*deep=*/ true);\n                }\n                self.zd_defined.clear();\n            }"))

    # (3) the Div/Mod arm: the predicate becomes const-map-aware.
    div_old = (
        "                if matches!(op, BinOp::Div | BinOp::Mod) {\n"
        "                    if is_int_literal_zero(rhs) {\n"
        "                        self.emit_const_zero_divisor(*span, bin_op_txt(*op));\n"
        "                    } else if is_float_literal_zero(rhs) && is_const_expr(lhs) {\n"
        "                        self.emit_const_zero_divisor(*span, bin_op_txt(*op));\n"
        "                    }\n"
        "                }")
    div_new = (
        "                if matches!(op, BinOp::Div | BinOp::Mod) {\n"
        "                    // Issue #758: the divisor predicate becomes const-map-\n"
        "                    // aware (#998 pattern): a named constant — or any constant\n"
        "                    // expression over them, incl. `len()` of a constant string\n"
        "                    // and constant conversions — dividing by zero rejects like\n"
        "                    // the literal twin. Shadow-checked against the live env;\n"
        "                    // names defined inside a deep-walked closure body fail open\n"
        "                    // (`self.zd_defined`). Runtime divisors stay out of scope.\n"
        "                    if divisor_int_fold_is_zero(rhs, &self.const_values, st, &self.zd_defined)\n"
        "                        || is_float_literal_zero(rhs) && is_const_expr(lhs)\n"
        "                        || divisor_float_fold_is_zero(rhs, &self.const_values, st, &self.zd_defined)\n"
        "                            && is_const_map_expr(lhs, &self.const_values, st, &self.zd_defined)\n"
        "                    {\n"
        "                        self.emit_const_zero_divisor(*span, bin_op_txt(*op));\n"
        "                    }\n"
        "                }")
    edits.append((ARITH, div_old, div_new))

    # (4) the compound-assign twin (`x /= zero`) — same map-aware predicate.
    assign_old = (
        "                if matches!(op, AssignOp::DivAssign | AssignOp::ModAssign)\n"
        "                    && values.len() == 1\n"
        "                    && is_int_literal_zero(&values[0])\n"
        "                {\n"
        "                    self.emit_const_zero_divisor(*span, assign_op_txt(*op));\n"
        "                }")
    assign_new = (
        "                if matches!(op, AssignOp::DivAssign | AssignOp::ModAssign)\n"
        "                    && values.len() == 1\n"
        "                    && (is_int_literal_zero(&values[0])\n"
        "                        || divisor_int_fold_is_zero(\n"
        "                            &values[0],\n"
        "                            &self.const_values,\n"
        "                            st,\n"
        "                            &self.zd_defined,\n"
        "                        ))\n"
        "                {\n"
        "                    self.emit_const_zero_divisor(*span, assign_op_txt(*op));\n"
        "                }")
    edits.append((ARITH, assign_old, assign_new))

    # (5) the array-length site: the SAME predicate, mirrored (#999 pattern).
    arr_old = (
        "    fn check_array_len(&mut self, st: Option<&FnState>, len: &Expr) {\n"
        "        let span = len.span();\n"
        "        // 1) A constant integer expression: must be non-negative.")
    arr_new = (
        "    fn check_array_len(&mut self, st: Option<&FnState>, len: &Expr) {\n"
        "        let span = len.span();\n"
        "        // Issue #758: a constant zero divisor inside the length\n"
        "        // expression is a Go compile error exactly like value position\n"
        "        // (`invalid operation: division by zero` in the array type) —\n"
        "        // the SAME predicate, mirrored onto the type-expression site\n"
        "        // (#999 pattern). Fail-open on non-constant lengths: those are\n"
        "        // the existing checks' business.\n"
        "        {\n"
        "            let empty_st = crate::model::FnState::default();\n"
        "            self.check_expr_const_zero_divisor(\n"
        "                st.unwrap_or(&empty_st),\n"
        "                len,\n"
        "            );\n"
        "        }\n"
        "        // 1) A constant integer expression: must be non-negative.")
    edits.append((EMIT, arr_old, arr_new))

    # (6) helpers appended before the unit-test module at the end of arith.rs
    edits.append((ARITH,
        "#[cfg(test)]\nmod lit_zero_tests {",
        HELPERS + "\n#[cfg(test)]\nmod lit_zero_tests {"))

    # (7) core variant: leave the len/conversion leaves out of the predicate
    if not with_len_leaves:
        edits.append((ARITH,
            "    if let Some(n) = const_len_str_value(bare, consts, defined, depth) {\n        return ConstFold::Val(n);\n    }\n    match bare {",
            "    match bare {"))
    if not with_float_conv:
        edits.append((ARITH,
            "    if divisor_float_conv_zero(e, consts, st, defined, 0) {\n        return true;\n    }\n    matches!(",
            "    matches!("))
    # (registry) the corpus join keys on tier0_rule_id — declare the
    # invariant class this code enforces (BUILDABLE: every accepted
    # program must emit go-buildable Go; the issue bodies cite it).
    edits.append(("compiler/crates/hy-diag/data/diagnostics.json",
                  '"code": "HYB-E6001",',
                  '"code": "HYB-E6001",\n      "tier0_rule_id": "BUILDABLE",'))

    return edits
