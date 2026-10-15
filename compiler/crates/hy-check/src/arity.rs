//! Call/return ARITY against the DECLARED shape (issue #152, slice 1).
//!
//! `hyb check` resolved names but never related an expression to the declared
//! shape it must match. The most ordinary mistake a beginner makes — the wrong
//! number of arguments — was accepted with 0 diagnostics and then died in
//! `go build`:
//!
//! ```hy
//! fn tambah(a int, b int) int { return a + b }
//! fn main() { fmt.println("{tambah(1)}") }
//! ```
//!
//! ```text
//! $ hyb check kurang.hy   -> kurang.hy: OK (0 diagnostics)
//! $ go build ./...        -> ./x.go:12:39: not enough arguments in call to tambah
//! ```
//!
//! Both directions and the return side are the same gap, so all three are here.
//!
//! Scope is deliberately narrow: only calls whose callee is a **file-local
//! function by bare name** are checked, and a call carrying a spread argument
//! (`f(xs...)`) is skipped entirely. Anything unknown stays permissive — a
//! false reject would be far worse than a missed one, since this pass can only
//! fire on a mismatch it is certain about.

use hy_syntax::{CallArg, Expr, FnDecl, FnResult, Stmt};

/// Why a call does not match its callee's declared parameters.
pub enum ArityError {
    TooFew { declared: usize, given: usize },
    TooMany { declared: usize, given: usize },
}

/// Check `args` against `decl`. `None` = nothing certain to report.
pub fn check_call_args(decl: &FnDecl, args: &[CallArg]) -> Option<ArityError> {
    // A spread argument forwards a slice; the count is not knowable here.
    if args.iter().any(|a| a.spread) {
        return None;
    }
    let declared = decl.params.len();
    let given = args.len();
    match given.cmp(&declared) {
        std::cmp::Ordering::Less => Some(ArityError::TooFew { declared, given }),
        std::cmp::Ordering::Greater => Some(ArityError::TooMany { declared, given }),
        std::cmp::Ordering::Equal => None,
    }
}

/// Why a `return` does not match the function's declared results.
pub enum ReturnError {
    TooFew { declared: usize, given: usize },
    TooMany { declared: usize, given: usize },
}

/// Check a `return` value list against the declared results.
///
/// A bare `return` (no values) in a named-result function is legal Go and is
/// therefore never reported. A spread never appears in a return list.
pub fn check_return_values(
    results: &[FnResult],
    values: &[Expr],
    is_bare: bool,
) -> Option<ReturnError> {
    if is_bare {
        return None;
    }
    let declared = results.len();
    let given = values.len();
    match given.cmp(&declared) {
        std::cmp::Ordering::Less => Some(ReturnError::TooFew { declared, given }),
        std::cmp::Ordering::Greater => Some(ReturnError::TooMany { declared, given }),
        std::cmp::Ordering::Equal => None,
    }
}

/// The bare-name callee of a call or task statement, if that is what it is.
pub fn bare_callee_expr(e: &Expr) -> Option<&str> {
    match e {
        Expr::Ident { name, .. } => Some(name.as_str()),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn decl_of(src: &str) -> FnDecl {
        let e = hy_syntax::parse_file("t.hy", src).expect("parse");
        let d = e.fns().next().unwrap().clone();
        d
    }

    fn args_of(src: &str) -> Vec<CallArg> {
        let e = hy_syntax::parse_file("t.hy", src).expect("parse");
        let f = e.fns().next().unwrap();
        let stmt = &f.body.as_ref().unwrap().stmts[0];
        match stmt {
            Stmt::Expr {
                expr: Expr::Call { args, .. },
                ..
            } => args.clone(),
            _ => panic!("expected a call statement"),
        }
    }

    #[test]
    fn too_few_arguments_is_reported() {
        let d = decl_of("fn tambah(a int, b int) int { return a + b }");
        let a = args_of("fn main() { tambah(1) }");
        match check_call_args(&d, &a) {
            Some(ArityError::TooFew { declared, given }) => {
                assert_eq!(declared, 2);
                assert_eq!(given, 1);
            }
            _ => panic!("expected TooFew"),
        }
    }

    #[test]
    fn too_many_arguments_is_reported() {
        let d = decl_of("fn satu(a int) int { return a }");
        let a = args_of("fn main() { satu(1, 2) }");
        assert!(matches!(
            check_call_args(&d, &a),
            Some(ArityError::TooMany { .. })
        ));
    }

    #[test]
    fn exact_count_is_silent() {
        let d = decl_of("fn tambah(a int, b int) int { return a + b }");
        let a = args_of("fn main() { tambah(1, 2) }");
        assert!(check_call_args(&d, &a).is_none());
    }

    #[test]
    fn spread_is_never_reported() {
        let d = decl_of("fn f(a int, b int, c int) int { return a }");
        let a = args_of("fn main() { f(1, xs...) }");
        assert!(
            check_call_args(&d, &a).is_none(),
            "a spread forwards a slice; the count is not knowable here"
        );
    }

    #[test]
    fn return_arity_both_directions() {
        let d = decl_of("fn dua() (int, int) {\n    return 1\n}");
        let stmt = {
            let e = hy_syntax::parse_file("t.hy", "fn dua() (int, int) {\n    return 1\n}")
                .expect("parse");
            let f = e.fns().next().unwrap();
            f.body.as_ref().unwrap().stmts[0].clone()
        };
        if let Stmt::Return { values, .. } = &stmt {
            assert!(matches!(
                check_return_values(&d.results, values, false),
                Some(ReturnError::TooFew { declared: 2, given: 1 })
            ));
        } else {
            panic!("expected a return");
        }
    }

    #[test]
    fn bare_return_is_legal_go_and_silent() {
        let d = decl_of("fn f() (code int) {\n    return\n}");
        assert!(check_return_values(&d.results, &[], true).is_none());
    }
}
