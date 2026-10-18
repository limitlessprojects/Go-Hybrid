//! tuple.rs — T23: a multi-value result is not a tuple (HYB-E2002).
//!
//! SPEC-LANGUAGE.md §2.8 (T23) pins `p := minmax(3, 9)` as a compile-fail
//! claim (`// reject: HYB-E2002`), and diagnostics.json registers the code,
//! but nothing raised it: the checker accepted the program with
//! 0 diagnostics and `go build` then failed with `assignment mismatch:
//! 1 variable but minmax returns 2 values`. Accepted programs must emit
//! buildable Go, so the binding is rejected here, before emission.
//!
//! Scope:
//! a define / `let` / `var` / plain `=` assignment whose right-hand side
//! contains a call declaring two or more results, with a target count that
//! differs from the result count. Covers direct top-level fn calls, fn-value
//! bindings / aliases (including type-aliased `fn` types), declared and
//! inline closures, user-defined struct methods, indirect `fn`-typed callees
//! (struct fields, array/slice/map elements, higher-order calls), and curated
//! foreign package calls. `?`-unwrapped calls stay permissive (unwrapping the
//! error produces a single value).

use crate::foreign;
use crate::model::{unwrap_parens, Checker, FnState, TypeShape};
use hy_diag::{Diagnostic, Span};
use hy_syntax::{AssignOp, Expr, Stmt, TypeExpr};

impl<'a> Checker<'a> {
    /// Pass-B entry (and closure-body entry in `closures.rs`), called once
    /// per statement.
    pub(crate) fn check_stmt_multi_value(&mut self, st: &FnState, stmt: &Stmt) {
        match stmt {
            Stmt::Local(l) if l.values.len() == 1 => {
                self.check_multi_value_bind(st, &l.values[0], l.names.len(), l.span);
            }
            Stmt::Local(l) if l.values.len() > 1 => {
                for v in &l.values {
                    self.check_multi_value_bind(st, v, 1, l.span);
                }
            }
            Stmt::Assign {
                targets,
                op: AssignOp::Assign,
                values,
                span,
            } if values.len() == 1 => {
                self.check_multi_value_bind(st, &values[0], targets.len(), *span);
            }
            Stmt::Assign {
                op: AssignOp::Assign,
                values,
                span,
                ..
            } if values.len() > 1 => {
                for v in values {
                    self.check_multi_value_bind(st, v, 1, *span);
                }
            }
            _ => {}
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

    /// `Some((callee, result count))` when `expr` is a direct or indirect call
    /// with a statically known result count (excluding `?`-unwrapped calls).
    fn static_result_count(&self, st: &FnState, expr: &Expr) -> Option<(String, usize)> {
        let Expr::Call { callee, .. } = unwrap_parens(expr) else {
            return None;
        };
        match unwrap_parens(callee) {
            Expr::Ident { name, .. } => {
                // 1. Local bindings (fn values, closures, parameters, aliases)
                if let Some(b) = self.lookup_binding(st, name) {
                    if let Some(ty) = &b.ty {
                        if let TypeExpr::Fn { results, .. } = self.resolve_type_alias(ty) {
                            return Some((name.clone(), results.len()));
                        }
                    }
                    let root = crate::freeze::resolve_alias(st, name);
                    if root != *name {
                        if let Some(rb) = self.lookup_binding(st, &root) {
                            if let Some(ty) = &rb.ty {
                                if let TypeExpr::Fn { results, .. } = self.resolve_type_alias(ty) {
                                    return Some((name.clone(), results.len()));
                                }
                            }
                        }
                        let rhead = Self::base_named_type_str(&root);
                        if let Some(decl) =
                            self.fns.get(root.as_str()).or_else(|| self.fns.get(rhead))
                        {
                            return Some((name.clone(), decl.results.len()));
                        }
                    }
                    return None;
                }
                // 2. Direct call to a file-local top-level function
                let head = Self::base_named_type_str(name);
                let decl = self.fns.get(name.as_str()).or_else(|| self.fns.get(head))?;
                Some((name.clone(), decl.results.len()))
            }
            Expr::FnLit { results, .. } => Some(("fn".to_string(), results.len())),
            Expr::Field {
                base,
                name: method_name,
                ..
            } => {
                let head = Self::base_named_type_str(method_name);
                // 1. Unshadowed package call (`strconv.parse_int(...)`, etc.)
                if let Expr::Ident { name: pkg, .. } = unwrap_parens(base) {
                    if self.lookup_binding(st, pkg).is_none() {
                        let full = format!("{pkg}.{head}");
                        if let Some(decl) = self.fns.get(full.as_str()) {
                            return Some((format!("{pkg}.{method_name}"), decl.results.len()));
                        }
                        if let Some(count) = foreign::package_result_count(pkg.as_str(), head) {
                            return Some((format!("{pkg}.{method_name}"), count));
                        }
                    }
                }
                // 2. User-defined struct method (`b.pair()`)
                let recv_base = self
                    .ty_of_expr(st, base)
                    .map(|t| self.resolve_type_alias(&t))
                    .and_then(|t| Self::base_named_type(&t))
                    .or_else(|| match self.shape_of_expr(st, base) {
                        TypeShape::Struct(Some(s)) => {
                            Some(Self::base_named_type_str(&s).to_string())
                        }
                        TypeShape::Pointer(inner) => match *inner {
                            TypeShape::Struct(Some(s)) => {
                                Some(Self::base_named_type_str(&s).to_string())
                            }
                            _ => None,
                        },
                        _ => None,
                    });
                if let Some(rb) = recv_base {
                    if let Some(decl) = self.method_decls.get(&(rb, head.to_string())) {
                        return Some((format!("{}.{method_name}", base.render()), decl.results.len()));
                    }
                }
                // 3. Struct field holding a fn value (`h.mm(3, 9)`)
                let callee_ty = self.ty_of_expr(st, callee)?;
                if let TypeExpr::Fn { results, .. } = self.resolve_type_alias(&callee_ty) {
                    return Some((callee.render(), results.len()));
                }
                None
            }
            other => {
                let callee_ty = self.ty_of_expr(st, other)?;
                if let TypeExpr::Fn { results, .. } = self.resolve_type_alias(&callee_ty) {
                    return Some((callee.render(), results.len()));
                }
                None
            }
        }
    }
}

