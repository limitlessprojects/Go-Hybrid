//! T23 (HYB-E2002): match arm guards with some(b) bindings and closure if-init conditions (#1905).
//!
//! Match arm guards and closure if-init conditions must evaluate single-value
//! constraints against their scoped bindings, not the unshadowed outer FnState.

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
        "expected only [{want}], got {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(cs.is_empty(), "expected 0 diagnostics, got {cs:?} for:\n{src}");
}

#[test]
fn match_guard_multi_value_fn_reject() {
    assert_code(
        r#"
fn pair() (int, int) { return 1, 2 }
fn main() {
    opt := some(pair)
    match opt {
        some(f) if f() == 1: {}
        none: {}
    }
}
"#,
        "HYB-E2002",
    );
}

#[test]
fn match_guard_shadows_single_value_reject() {
    assert_code(
        r#"
fn single() int { return 1 }
fn pair() (int, int) { return 1, 2 }
fn main() {
    f := single
    opt := some(pair)
    match opt {
        some(f) if f() == 1: {}
        none: {}
    }
}
"#,
        "HYB-E2002",
    );
}

#[test]
fn closure_if_init_binds_multi_value_reject() {
    assert_code(
        r#"
fn single() int { return 1 }
fn pair() (int, int) { return 1, 2 }
fn main() {
    f := single
    cb := fn() {
        if f := pair; f() == 1 {}
    }
    cb()
}
"#,
        "HYB-E2002",
    );
}

#[test]
fn match_guard_single_value_fn_ok() {
    assert_ok(
        r#"
fn single() int { return 1 }
fn main() {
    opt := some(single)
    match opt {
        some(f) if f() == 1: {}
        none: {}
    }
}
"#,
    );
}

#[test]
fn closure_if_init_single_value_ok() {
    assert_ok(
        r#"
fn single() int { return 1 }
fn main() {
    cb := fn() {
        if f := single; f() == 1 {}
    }
    cb()
}
"#,
    );
}
