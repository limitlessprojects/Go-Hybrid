//! Regression tests for type-elided struct literals (`{...}`) on zero-field user
//! structs (`struct empty {}`), foreign structs (`http.server`), and inline
//! `struct{...}` types (`elided_literal.rs`, `HYB-E1002`, GUARANTEES Row 13).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|d| panic!("must parse: {d:?}"));
    check_file(&ast)
}

fn errors(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect()
}

fn assert_rejects_e1002(src: &str, needle: &str) {
    let r = check(src);
    let errs = errors(&r);
    assert_eq!(r.verdict, Verdict::Reject, "must reject:\n{src}");
    assert!(
        errs.iter().any(|e| e.starts_with("HYB-E1002") && e.contains(needle)),
        "expected HYB-E1002 mentioning `{needle}` for:\n{src}\ngot {errs:?}"
    );
}

fn assert_accepts(src: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "must accept:\n{src}\ngot {:?}",
        errors(&r)
    );
}

#[test]
fn elided_foreign_struct_literals_reject_duplicate_fields() {
    for (src, needle) in [
        (
            "import \"net/http\"\nfn main() {\n    xs := []http.server{{addr: \":80\", addr: \":81\"}}\n    _ = xs\n}\n",
            "duplicate field `addr` in `http.server` literal",
        ),
        (
            "import \"net/http\"\nfn main() {\n    xs := []*http.server{{addr: \":80\", addr: \":81\"}}\n    _ = xs\n}\n",
            "duplicate field `addr` in `http.server` literal",
        ),
        (
            "import \"net/http\"\nfn main() {\n    m := map[string]http.server{\"a\": {addr: \":80\", addr: \":81\"}}\n    _ = m\n}\n",
            "duplicate field `addr` in `http.server` literal",
        ),
    ] {
        assert_rejects_e1002(src, needle);
    }
}

#[test]
fn elided_zero_field_user_struct_literals_reject_unknown_and_duplicate_fields() {
    for (src, needle) in [
        (
            "struct empty {}\nfn main() {\n    xs := []empty{{nope: 1}}\n    _ = xs\n}\n",
            "unknown field `nope` in `empty` literal",
        ),
        (
            "struct empty {}\nfn main() {\n    xs := []*empty{{nope: 1}}\n    _ = xs\n}\n",
            "unknown field `nope` in `empty` literal",
        ),
        (
            "struct empty {}\nfn main() {\n    m := map[string]empty{\"a\": {nope: 1}}\n    _ = m\n}\n",
            "unknown field `nope` in `empty` literal",
        ),
        (
            "struct empty {}\nfn main() {\n    xs := []empty{{nope: 1, nope: 2}}\n    _ = xs\n}\n",
            "duplicate field `nope` in `empty` literal",
        ),
    ] {
        assert_rejects_e1002(src, needle);
    }
}

#[test]
fn valid_zero_field_and_foreign_elided_struct_literals_stay_accepted() {
    assert_accepts(
        "import \"net/http\"\n\
         struct empty {}\n\
         struct marker[t] {}\n\
         type empty_ptr = *empty\n\
         fn main() {\n\
             es := []empty{{}, {}}\n\
             ep := []empty_ptr{{}}\n\
             ms := []marker[int]{{}}\n\
             em := map[string]empty{\"a\": {}}\n\
             svs := []http.server{{addr: \":8080\"}}\n\
             _ = es\n\
             _ = ep\n\
             _ = ms\n\
             _ = em\n\
             _ = svs\n\
         }\n",
    );
}

#[test]
fn elided_inline_struct_named_type_rejects_unknown_and_duplicate_fields() {
    let mut ast = parse_file(
        "inline.hy",
        "fn main() {\n    xs := []placeholder{{nope: 1, x: 2, x: 3}}\n    _ = xs\n}\n",
    )
    .expect("must parse");
    if let hy_syntax::Decl::Fn(f) = &mut ast.decls[0] {
        if let Some(b) = &mut f.body {
            if let hy_syntax::Stmt::Local(l) = &mut b.stmts[0] {
                if let hy_syntax::Expr::SliceLit {
                    elem: Some(elem_ty),
                    ..
                } = &mut l.values[0]
                {
                    if let hy_syntax::TypeExpr::Named { name, .. } = elem_ty.as_mut() {
                        *name = "struct{ x int }".to_string();
                    }
                }
            }
        }
    }
    let r = check_file(&ast);
    let errs = errors(&r);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        errs.iter()
            .any(|e| e.contains("unknown field `nope` in `struct{ x int }` literal")),
        "got {errs:?}"
    );
    assert!(
        errs.iter()
            .any(|e| e.contains("duplicate field `x` in `struct{ x int }` literal")),
        "got {errs:?}"
    );
}

