//! Regression tests for eager argument evaluation at `defer` sites mutating
//! named result bindings even when argument variables are rebound later.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn verify(src: &str, expected: Option<&str>) {
    let ast = parse_file("defer_eager_arg_provenance.hy", src).expect("fixture must parse");
    let result = check_file(&ast);
    if let Some(code) = expected {
        assert_eq!(result.verdict, Verdict::Reject, "{:#?}", result.diagnostics);
        assert!(
            result.diagnostics.iter().any(|d| d.code == code),
            "expected {code}, got {:#?}",
            result.diagnostics
        );
    } else {
        assert_eq!(result.verdict, Verdict::Accept, "{:#?}", result.diagnostics);
        assert!(result.diagnostics.is_empty(), "{:#?}", result.diagnostics);
    }
}

#[test]
fn defer_eager_arg_retains_sharing_after_source_rebind_rejects() {
    verify(
        r#"
struct holder {
    s []int
}

fn dup() (a holder, b holder) {
    x := holder{s: []int{1}}
    b = x
    defer fn(v holder) {
        a = v
    }(x)
    x = holder{s: []int{9}}
    return
}

fn worker(h holder) {}

fn main() {
    h1, h2 := dup()
    task worker(move h1)
    h2.s[0] = 2
}
"#,
        Some("HYB-E3003"),
    );
}

#[test]
fn defer_eager_arg_retains_const_origin_after_source_rebind_rejects() {
    verify(
        r#"
struct holder {
    s []int
}

const shared_const = holder{s: []int{0}}

fn leak() (a holder) {
    c := shared_const
    defer fn(v holder) {
        a = v
    }(c)
    c = holder{s: []int{9}}
    return
}

fn main() {
    h := leak()
    h.s[0] = 1
}
"#,
        Some("HYB-E3006"),
    );
}

#[test]
fn defer_eager_arg_retains_borrow_taint_after_source_rebind_rejects() {
    verify(
        r#"
type int_ptr = *int

fn escape(fallback *int) (out int_ptr) {
    x := 1
    p := &x
    defer fn(q int_ptr) {
        out = q
    }(p)
    p = fallback
    return fallback
}
"#,
        Some("HYB-E3104"),
    );
}

#[test]
fn defer_eager_non_borrow_arg_before_source_borrow_rebind_accepts() {
    verify(
        r#"
type int_ptr = *int

fn ok_eager(fallback *int) (out int_ptr) {
    x := 1
    p := fallback
    defer fn(q int_ptr) {
        out = q
    }(p)
    p = &x
    return fallback
}
"#,
        None,
    );
}
