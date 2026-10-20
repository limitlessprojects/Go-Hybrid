//! #1015 — Multi-value call argument type checking and method/closure arity.
//!
//! A lone multi-value call expands across its caller's argument list:
//! `f(g())` for `fn f(a int, b int)` and `fn g() (int, int)`.
//!
//! Three defects previously violated Row 13 (BUILDABLE):
//! 1. `check_user_fn_arg_types` and `check_method_arg_types` skipped type
//!    checking entirely when `multi_value_arg_count.is_some()`, allowing
//!    mismatched argument types to slip through with 0 diagnostics.
//! 2. `check_method_arity` compared `args.len()` directly without accounting
//!    for multi-value argument expansion, falsely rejecting valid calls like
//!    `s.m(two())` with HYB-E1002.
//! 3. Local closures `cb(two())` similarly falsely rejected valid calls with
//!    HYB-E1002 arity errors.

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

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(
        cs.is_empty(),
        "expected 0 diagnostics, got {cs:?} for source:\n{src}"
    );
}

fn assert_e1002(src: &str) {
    let cs = codes(src);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {cs:?} for source:\n{src}"
    );
}

const COMMON_DEFS: &str = r#"
fn two_ints() (int, int) {
    return 1, 2
}

fn two_strings() (string, string) {
    return "a", "b"
}

fn three_ints() (int, int, int) {
    return 1, 2, 3
}

struct receiver {}

fn (r receiver) takes_two(a int, b int) {}
"#;

#[test]
fn user_fn_multi_value_matching_types_accepted() {
    let src = format!(
        r#"{COMMON_DEFS}
fn takes_two_ints(a int, b int) {{}}

fn main() {{
    takes_two_ints(two_ints())
}}
"#
    );
    assert_ok(&src);
}

#[test]
fn user_fn_multi_value_type_mismatch_rejected() {
    let src = format!(
        r#"{COMMON_DEFS}
fn takes_two_ints(a int, b int) {{}}

fn main() {{
    takes_two_ints(two_strings())
}}
"#
    );
    assert_e1002(&src);
}

#[test]
fn method_multi_value_matching_types_accepted() {
    let src = format!(
        r#"{COMMON_DEFS}
fn main() {{
    var r receiver
    r.takes_two(two_ints())
}}
"#
    );
    assert_ok(&src);
}

#[test]
fn method_multi_value_type_mismatch_rejected() {
    let src = format!(
        r#"{COMMON_DEFS}
fn main() {{
    var r receiver
    r.takes_two(two_strings())
}}
"#
    );
    assert_e1002(&src);
}

#[test]
fn method_multi_value_arity_mismatch_rejected() {
    let src = format!(
        r#"{COMMON_DEFS}
fn main() {{
    var r receiver
    r.takes_two(three_ints())
}}
"#
    );
    assert_e1002(&src);
}

#[test]
fn task_spawn_multi_value_matching_types_accepted() {
    let src = format!(
        r#"{COMMON_DEFS}
fn takes_two_ints(a int, b int) {{}}

fn main() {{
    task takes_two_ints(two_ints())
}}
"#
    );
    assert_ok(&src);
}

#[test]
fn task_spawn_multi_value_type_mismatch_rejected() {
    let src = format!(
        r#"{COMMON_DEFS}
fn takes_two_ints(a int, b int) {{}}

fn main() {{
    task takes_two_ints(two_strings())
}}
"#
    );
    assert_e1002(&src);
}

#[test]
fn task_spawn_multi_value_arity_mismatch_rejected() {
    let src = format!(
        r#"{COMMON_DEFS}
fn takes_two_ints(a int, b int) {{}}

fn main() {{
    task takes_two_ints(three_ints())
}}
"#
    );
    assert_e1002(&src);
}

#[test]
fn closure_multi_value_matching_types_accepted() {
    let src = format!(
        r#"{COMMON_DEFS}
fn main() {{
    cb := fn(a int, b int) {{}}
    cb(two_ints())
}}
"#
    );
    assert_ok(&src);
}

#[test]
fn closure_multi_value_arity_mismatch_rejected() {
    let src = format!(
        r#"{COMMON_DEFS}
fn main() {{
    cb := fn(a int, b int) {{}}
    cb(three_ints())
}}
"#
    );
    assert_e1002(&src);
}
