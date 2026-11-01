//! Regression tests for multi-return storage sharing introduced via `defer`
//! mutating named result bindings after `return` assigns result slots.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn verify(src: &str, expected: Option<&str>) {
    let ast = parse_file("defer_named_result_shared.hy", src).expect("fixture must parse");
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
fn defer_assign_into_named_result_on_bare_return_rejects_alias_write() {
    verify(
        r#"
struct holder {
    s []int
}

fn dup() (a holder, b holder) {
    x := holder{s: []int{1}}
    defer fn() {
        a = x
    }()
    b = x
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
fn defer_overwrites_explicit_return_into_shared_pair_rejects() {
    verify(
        r#"
struct holder {
    s []int
}

fn dup() (a holder, b holder) {
    x := holder{s: []int{1}}
    defer fn() {
        a = x
    }()
    return holder{s: []int{99}}, x
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
fn defer_local_closure_and_lifo_chain_into_named_result_rejects() {
    verify(
        r#"
struct holder {
    s []int
}

fn dup_via_local_closure() (a holder, b holder) {
    x := holder{s: []int{1}}
    set_a := fn() {
        a = x
    }
    defer set_a()
    b = x
    return
}

fn worker(h holder) {}

fn main() {
    h1, h2 := dup_via_local_closure()
    task worker(move h1)
    h2.s[0] = 2
}
"#,
        Some("HYB-E3003"),
    );

    verify(
        r#"
struct holder {
    s []int
}

fn dup_lifo() (a holder, b holder) {
    var tmp holder = holder{s: []int{0}}
    defer fn() {
        a = tmp
    }()
    defer fn() {
        tmp = b
    }()
    return holder{s: []int{1}}, holder{s: []int{2}}
}

fn worker(h holder) {}

fn main() {
    h1, h2 := dup_lifo()
    task worker(move h1)
    h2.s[0] = 9
}
"#,
        Some("HYB-E3003"),
    );
}

#[test]
fn defer_field_store_and_copy_into_named_result_rejects() {
    verify(
        r#"
struct holder {
    s []int
}

fn dup_field() (a holder, b holder) {
    x := []int{1}
    a = holder{s: []int{0}}
    b = holder{s: x}
    defer fn() {
        a.s = x
    }()
    return
}

fn worker(h holder) {}

fn main() {
    h1, h2 := dup_field()
    task worker(move h1)
    h2.s[0] = 7
}
"#,
        Some("HYB-E3003"),
    );
}

#[test]
fn defer_independent_fresh_value_into_named_result_accepts() {
    verify(
        r#"
struct holder {
    s []int
}

fn disjoint() (a holder, b holder) {
    x := holder{s: []int{1}}
    defer fn() {
        a = holder{s: []int{2}}
    }()
    b = x
    return
}

fn worker(h holder) {}

fn main() {
    h1, h2 := disjoint()
    task worker(move h1)
    h2.s[0] = 3
}
"#,
        None,
    );
}
