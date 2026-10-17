//! tuple.rs — T23: a multi-value result is not a tuple (HYB-E2002).
//!
//! SPEC-LANGUAGE.md §2.8 (T23) pins `p := minmax(3, 9)` as a compile-fail
//! claim (`// reject: HYB-E2002`), and diagnostics.json registers the code,
//! but nothing raised it: the checker accepted the program with
//! 0 diagnostics and `go build` then failed with `assignment mismatch:
//! 1 variable but minmax returns 2 values`. Accepted programs must emit
//! buildable Go, so the binding is rejected here, before emission.
//!
//! Scope (narrow on purpose — fire only on a mismatch we are certain of):
//! a define / `let` / `var` / plain `=` assignment whose single right-hand
//! side is a direct call to a file-local, non-shadowed top-level function
//! declaring two or more results, with a target count that differs from
//! the result count. `?`-unwrapped calls, method calls, package calls,
//! closures and foreign bindings stay permissive (their result counts are
//! not known here).

use crate::model::{unwrap_parens, Binding, Checker, FnState, TypeShape};
use hy_diag::{Diagnostic, Span};
use hy_syntax::{AssignOp, Block, Expr, ForKind, InterpPart, Place, PlaceKind, SelectArmKind, Stmt};

impl<'a> Checker<'a> {
    /// Pass-B entry, called once per flattened statement (nested block
    /// bodies are separate flat entries, so no recursion here).
    pub(crate) fn check_stmt_multi_value(&mut self, st: &FnState, stmt: &Stmt) {
        match stmt {
            Stmt::Local(l) if l.values.len() == 1 => {
                self.check_multi_value_bind(st, &l.values[0], l.names.len(), l.span);
            }
            Stmt::Assign {
                targets,
                op: AssignOp::Assign,
                values,
                span,
            } if values.len() == 1 => {
                self.check_multi_value_bind(st, &values[0], targets.len(), *span);
            }
            Stmt::For {
                kind: ForKind::Iterate { expr, .. },
                span,
                ..
            } => {
                self.check_range_iterable(st, expr, *span);
            }
            _ => {}
        }
        self.check_stmt_closures_range_iterable(st, stmt, false);
    }

    /// Classify the iterable expression of `for x in e` (SPEC-LANGUAGE LX13,
    /// issue #411): slices, arrays, strings, maps, channels, and L3 borrow
    /// views (`BorrowHandle`) are iterable; concrete non-iterables are
    /// rejected with HYB-E2005 (rule LX13), and unproven `Unknown` shapes
    /// fail closed with HYB-E3005 (O4.9).
    pub(crate) fn check_range_iterable(&mut self, st: &FnState, expr: &Expr, span: Span) {
        if self.is_Void_or_NonIterable(st, expr, span) {
            return;
        }
    }

    #[allow(non_snake_case)]
    fn is_Void_or_NonIterable(&mut self, st: &FnState, expr: &Expr, span: Span) -> bool {
        if self.is_known_void_call_for_range(st, expr) {
            let msg = format!(
                "`for ... in` cannot range over `{}` (void call) — `for x in e` iterates slices, arrays, strings, maps, and channels (LX13)",
                expr.render()
            );
            let notes = vec![
                "why: a call with no return value cannot be iterated in Go (`used as value` / `cannot range over`); accepted programs must emit buildable Go (issue #411)".to_string(),
                "fix: iterate over a slice (`[]T`), array (`[N]T`), string, map (`map[K]V`), channel (`chan T`), or integer range (`for i in 0..n`)".to_string(),
            ];
            self.diags.push(
                Diagnostic::error("HYB-E2009", self.path.clone(), span, msg)
                    .with_rule("LX13")
                    .with_notes(notes),
            );
            return true;
        }
        match self.shape_of_expr(st, expr) {
            TypeShape::Slice(_)
            | TypeShape::Array(_)
            | TypeShape::Str
            | TypeShape::Map(_, _)
            | TypeShape::Chan(_)
            | TypeShape::BorrowHandle(_)
            | TypeShape::Unknown => false,
            TypeShape::Pointer(inner) if matches!(*inner, TypeShape::Array(_) | TypeShape::Unknown) => false,
            other => {
                let ty_str = self
                    .ty_of_expr(st, expr)
                    .map(|t| t.render())
                    .unwrap_or_else(|| other.render());
                let msg = format!(
                    "`for ... in` iterates slices, arrays, strings, maps, and channels — not `{}` ({}) (LX13)",
                    expr.render(),
                    ty_str
                );
                let notes = vec![
                    format!(
                        "why: Go rejects `range {}` (`cannot range over`), so an accepted program would emit unbuildable Go (issue #411)",
                        expr.render()
                    ),
                    "fix: iterate over a slice, array, string, map, or channel, or use an integer range `for i in 0..n`".to_string(),
                ];
                self.diags.push(
                    Diagnostic::error("HYB-E2009", self.path.clone(), span, msg)
                        .with_rule("LX13")
                        .with_notes(notes),
                );
                true
            }
        }
    }

    fn is_known_void_call_for_range(&self, st: &FnState, expr: &Expr) -> bool {
        let Expr::Call { callee, .. } = unwrap_parens(expr) else {
            return false;
        };
        match unwrap_parens(callee) {
            Expr::Ident { name, .. } => {
                if self.lookup_binding(st, name).is_some() {
                    return false;
                }
                self.fns
                    .get(name.as_str())
                    .map(|decl| decl.results.is_empty())
                    .unwrap_or(false)
            }
            Expr::Field { base, name, .. } => {
                let recv_shape = self.shape_of_expr(st, base);
                let recv_base = match recv_shape {
                    TypeShape::Struct(Some(s)) => Some(s),
                    TypeShape::Pointer(inner) => match *inner {
                        TypeShape::Struct(Some(s)) => Some(s),
                        _ => None,
                    },
                    _ => None,
                };
                if let Some(rname) = recv_base {
                    if let Some(decl) = self.method_decls.get(&(rname, name.clone())) {
                        return decl.results.is_empty();
                    }
                }
                false
            }
            _ => false,
        }
    }

    fn check_stmt_closures_range_iterable(&mut self, st: &FnState, stmt: &Stmt, deep: bool) {
        match stmt {
            Stmt::Local(l) => {
                for v in &l.values {
                    self.check_expr_closures_range_iterable(st, v);
                }
            }
            Stmt::Assign {
                targets, values, ..
            } => {
                for t in targets {
                    self.check_place_closures_range_iterable(st, t);
                }
                for v in values {
                    self.check_expr_closures_range_iterable(st, v);
                }
            }
            Stmt::Expr { expr, .. } => self.check_expr_closures_range_iterable(st, expr),
            Stmt::Send { chan, value, .. } => {
                self.check_expr_closures_range_iterable(st, chan);
                self.check_expr_closures_range_iterable(st, value);
            }
            Stmt::If {
                init,
                cond,
                then,
                otherwise,
                ..
            } => {
                if deep {
                    if let Some(i) = init {
                        self.check_stmt_closures_range_iterable(st, i, true);
                    }
                }
                self.check_expr_closures_range_iterable(st, cond);
                if deep {
                    for s in &then.stmts {
                        self.check_stmt_closures_range_iterable(st, s, true);
                    }
                    if let Some(o) = otherwise {
                        self.check_stmt_closures_range_iterable(st, o, true);
                    }
                }
            }
            Stmt::For {
                kind, body, span, ..
            } => {
                if deep {
                    if let ForKind::Iterate { expr, .. } = kind {
                        self.check_range_iterable(st, expr, *span);
                    }
                }
                match kind {
                    ForKind::While { cond } => self.check_expr_closures_range_iterable(st, cond),
                    ForKind::Range { lo, hi, .. } => {
                        self.check_expr_closures_range_iterable(st, lo);
                        self.check_expr_closures_range_iterable(st, hi);
                    }
                    ForKind::Iterate { expr, .. } => {
                        self.check_expr_closures_range_iterable(st, expr)
                    }
                    ForKind::Infinite => {}
                }
                if deep {
                    for s in &body.stmts {
                        self.check_stmt_closures_range_iterable(st, s, true);
                    }
                }
            }
            Stmt::Return { values, .. } => {
                for v in values {
                    self.check_expr_closures_range_iterable(st, v);
                }
            }
            Stmt::Task { callee, args, .. } => {
                self.check_expr_closures_range_iterable(st, callee);
                for a in args {
                    self.check_expr_closures_range_iterable(st, &a.value);
                }
            }
            Stmt::Managed { body, .. } => {
                if deep {
                    for s in &body.stmts {
                        self.check_stmt_closures_range_iterable(st, s, true);
                    }
                }
            }
            Stmt::Match { subject, arms, .. } => {
                self.check_expr_closures_range_iterable(st, subject);
                for arm in arms {
                    if let Some(g) = &arm.guard {
                        self.check_expr_closures_range_iterable(st, g);
                    }
                    if deep {
                        for s in &arm.body.stmts {
                            self.check_stmt_closures_range_iterable(st, s, true);
                        }
                    }
                }
            }
            Stmt::Defer { call, .. } => self.check_expr_closures_range_iterable(st, call),
            Stmt::Break { .. } | Stmt::Continue { .. } => {}
            Stmt::Block { block, .. } => {
                if deep {
                    for s in &block.stmts {
                        self.check_stmt_closures_range_iterable(st, s, true);
                    }
                }
            }
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    match &arm.kind {
                        SelectArmKind::Send { chan, value } => {
                            self.check_expr_closures_range_iterable(st, chan);
                            self.check_expr_closures_range_iterable(st, value);
                        }
                        SelectArmKind::Recv { chan, .. } => {
                            self.check_expr_closures_range_iterable(st, chan);
                        }
                        SelectArmKind::Default => {}
                    }
                    if deep {
                        for s in &arm.body.stmts {
                            self.check_stmt_closures_range_iterable(st, s, true);
                        }
                    }
                }
            }
        }
    }

    fn check_place_closures_range_iterable(&mut self, st: &FnState, p: &Place) {
        match &p.kind {
            PlaceKind::Ident(_) => {}
            PlaceKind::Index { base, index } => {
                self.check_place_closures_range_iterable(st, base);
                self.check_expr_closures_range_iterable(st, index);
            }
            PlaceKind::Field { base, .. } | PlaceKind::AddrOf { base } => {
                self.check_place_closures_range_iterable(st, base);
            }
        }
    }

    fn check_expr_closures_range_iterable(&mut self, st: &FnState, expr: &Expr) {
        match expr {
            Expr::FnLit { params, body, .. } => {
                let mut cst = st.clone();
                cst.scope_push();
                for p in params {
                    let shape = self.shape_of_type(&p.ty);
                    cst.env_define(
                        &p.name,
                        Binding {
                            shape,
                            ty: Some(p.ty.clone()),
                            immutable: false,
                            managed_taint: false,
                            moved: None,
                            non_owned_origin: None,
                            slot_loan: false,
                        },
                    );
                }
                self.check_closure_block_range_iterable(&mut cst, body);
                cst.scope_pop();
            }
            Expr::Interp { parts, .. } => {
                for p in parts {
                    if let InterpPart::Expr(e) = p {
                        self.check_expr_closures_range_iterable(st, e);
                    }
                }
            }
            Expr::Binary { lhs, rhs, .. } => {
                self.check_expr_closures_range_iterable(st, lhs);
                self.check_expr_closures_range_iterable(st, rhs);
            }
            Expr::Unary { operand, .. }
            | Expr::AddrOf { operand, .. }
            | Expr::Move { operand, .. }
            | Expr::Try { operand, .. } => self.check_expr_closures_range_iterable(st, operand),
            Expr::Receive { chan, .. } => self.check_expr_closures_range_iterable(st, chan),
            Expr::Index { base, index, .. } => {
                self.check_expr_closures_range_iterable(st, base);
                self.check_expr_closures_range_iterable(st, index);
            }
            Expr::Slice { base, lo, hi, .. } => {
                self.check_expr_closures_range_iterable(st, base);
                if let Some(lo) = lo {
                    self.check_expr_closures_range_iterable(st, lo);
                }
                if let Some(hi) = hi {
                    self.check_expr_closures_range_iterable(st, hi);
                }
            }
            Expr::Field { base, .. } => self.check_expr_closures_range_iterable(st, base),
            Expr::Call { callee, args, .. } => {
                self.check_expr_closures_range_iterable(st, callee);
                for a in args {
                    self.check_expr_closures_range_iterable(st, &a.value);
                }
            }
            Expr::StructLit { fields, .. } => {
                for f in fields {
                    self.check_expr_closures_range_iterable(st, &f.value);
                }
            }
            Expr::SliceLit { items, .. } => {
                for it in items {
                    self.check_expr_closures_range_iterable(st, it);
                }
            }
            Expr::MapLit { entries, .. } => {
                for (k, v) in entries {
                    self.check_expr_closures_range_iterable(st, k);
                    self.check_expr_closures_range_iterable(st, v);
                }
            }
            Expr::Paren { inner, .. } => self.check_expr_closures_range_iterable(st, inner),
            Expr::Ident { .. } | Expr::Lit { .. } => {}
        }
    }

    fn check_closure_block_range_iterable(&mut self, cst: &mut FnState, block: &Block) {
        for s in &block.stmts {
            if let Stmt::Local(l) = s {
                for (i, name) in l.names.iter().enumerate() {
                    let shape = if let Some(ty) = &l.ty {
                        self.shape_of_type(ty)
                    } else if let Some(v) = l.values.get(i).or_else(|| l.values.first()) {
                        self.shape_of_expr(cst, v)
                    } else {
                        TypeShape::Unknown
                    };
                    cst.env_define(
                        name,
                        Binding {
                            shape,
                            ty: l.ty.clone(),
                            immutable: false,
                            managed_taint: false,
                            moved: None,
                            non_owned_origin: None,
                            slot_loan: false,
                        },
                    );
                }
            }
            self.check_stmt_closures_range_iterable(cst, s, true);
        }
    }

    fn check_multi_value_bind(&mut self, st: &FnState, value: &Expr, targets: usize, span: Span) {
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
    }

    /// `Some((callee, result count))` when `expr` is a direct call to a
    /// top-level function of this file that no local binding shadows.
    fn static_result_count(&self, st: &FnState, expr: &Expr) -> Option<(String, usize)> {
        let Expr::Call { callee, .. } = unwrap_parens(expr) else {
            return None;
        };
        let Expr::Ident { name, .. } = unwrap_parens(callee) else {
            return None;
        };
        if self.lookup_binding(st, name).is_some() {
            return None; // shadowed by a local — not a static fn call
        }
        let decl = self.fns.get(name.as_str())?;
        Some((name.clone(), decl.results.len()))
    }
}

#[cfg(test)]
mod tests {
    use crate::check_file;
    use hy_syntax::parse_file;

    fn codes(src: &str) -> Vec<String> {
        let file = parse_file("test.hy", src).expect("parse");
        let checked = check_file(&file);
        checked.diagnostics.into_iter().map(|d| d.code).collect()
    }

    #[test]
    fn reject_for_in_over_int_bool_and_struct() {
        let src = r#"import "fmt"

struct box {
    n int
}

fn main() {
    x := 5
    for v in x {
        fmt.println(v)
    }
    b := true
    for v in b {
        fmt.println(v)
    }
    s := box{n: 1}
    for v in s {
        fmt.println(v)
    }
}
"#;
        assert_eq!(
            codes(src),
            vec!["HYB-E2009", "HYB-E2009", "HYB-E2009"]
        );
    }

    #[test]
    fn reject_for_in_over_non_iterable_call_result_and_void_call() {
        let src = r#"import "fmt"

fn count() int {
    return 3
}

fn noop() {}

fn main() {
    for v in count() {
        fmt.println(v)
    }
    for v in noop() {
        fmt.println(v)
    }
}
"#;
        assert_eq!(codes(src), vec!["HYB-E2009", "HYB-E2009"]);
    }

    #[test]
    fn reject_for_in_inside_closure_and_keep_unknown_permissive() {
        let src = r#"import "fmt"

fn main() {
    f := fn() {
        n := 10
        for v in n {
            fmt.println(v)
        }
    }
    f()
    for v in mystery_source() {
        fmt.println(v)
    }
}
"#;
        assert_eq!(codes(src), vec!["HYB-E2009"]);
    }

    #[test]
    fn accept_for_in_over_slice_array_string_map_chan_and_range() {
        let src = r#"import "fmt"

fn main() {
    xs := []int{1, 2}
    for v in xs {
        fmt.println(v)
    }
    var arr [2]int
    for v in arr {
        fmt.println(v)
    }
    for r in "hi" {
        fmt.println(r)
    }
    m := map[string]int{"a": 1}
    for k in m {
        fmt.println(k)
    }
    ch := make(chan int, 1)
    for v in ch {
        fmt.println(v)
    }
    for i in 0..3 {
        fmt.println(i)
    }
}
"#;
        assert!(codes(src).is_empty());
    }
}

