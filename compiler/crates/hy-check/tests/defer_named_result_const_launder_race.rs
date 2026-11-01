//! Regression tests for file-level `const` composite provenance when a
//! `defer` statement writes a `const` alias into a named result binding.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn verify(src: &str, expected: Option<&str>) {
    let ast = parse_file("defer_named_result_const.hy", src).expect("fixture must parse");
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
fn defer_laundering_file_const_via_bare_return_rejects_write_and_move() {
    verify(
        r#"
struct holder {
    s []int
}

const shared_const = holder{s: []int{0}}

fn leak_const() (a holder) {
    defer fn() {
        a = shared_const
    }()
    return
}

fn main() {
    h := leak_const()
    h.s[0] = 1
}
"#,
        Some("HYB-E3006"),
    );

    verify(
        r#"
struct holder {
    s []int
}

const shared_const = holder{s: []int{0}}

fn leak_const() (a holder) {
    defer fn() {
        a = shared_const
    }()
    return
}

fn worker(h holder) {}

fn main() {
    h := leak_const()
    task worker(move h)
}
"#,
        Some("HYB-E3004"),
    );
}

#[test]
fn defer_overwriting_explicit_return_with_file_const_rejects() {
    verify(
        r#"
struct holder {
    s []int
}

const shared_const = holder{s: []int{0}}

fn leak_const() (a holder) {
    defer fn() {
        a = shared_const
    }()
    return holder{s: []int{99}}
}

fn main() {
    h := leak_const()
    h.s[0] = 1
}
"#,
        Some("HYB-E3006"),
    );
}

#[test]
fn defer_local_closure_and_lifo_chain_laundering_const_rejects() {
    verify(
        r#"
struct holder {
    s []int
}

const shared_const = holder{s: []int{0}}

fn leak_field() (a holder) {
    a = holder{s: []int{1}}
    set_s := fn() {
        a.s = shared_const.s
    }
    defer set_s()
    return
}

fn main() {
    h := leak_field()
    h.s[0] = 2
}
"#,
        Some("HYB-E3006"),
    );

    verify(
        r#"
struct holder {
    s []int
}

const shared_const = holder{s: []int{0}}

fn leak_lifo() (a holder) {
    var tmp holder = holder{s: []int{1}}
    defer fn() {
        a = tmp
    }()
    defer fn() {
        tmp = shared_const
    }()
    return holder{s: []int{2}}
}

fn main() {
    h := leak_lifo()
    h.s[0] = 3
}
"#,
        Some("HYB-E3006"),
    );
}
