//! issue_prelude_name_guards_4695.rs — Issue #4695: prelude helper names hy_pop /
//! hy_n5_make / set must not collide with user declarations (Row 13 BUILDABLE).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|d| panic!("must parse: {d:?}"));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn error_messages(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.message.clone())
        .collect()
}

#[test]
fn reproducer_a_fn_set_with_set_use_rejected() {
    let src = r#"
fn set() int {
    return 1
}

fn main() {
    seen := set[int]{}
    seen.insert(3)
    fmt.println("{set()} {seen.len()}")
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(error_messages(&r).iter().any(|m| m.contains("named `set`")));
}

#[test]
fn reproducer_b_fn_hypop_rejected() {
    let src = r#"
fn hy_pop() int {
    return 7
}

fn main() {
    stack := []int{1, 2, 3}
    popped := stack.pop()
    fmt.println("{hy_pop()} {popped}")
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(error_messages(&r).iter().any(|m| m.contains("named `hy_pop`")));
}

#[test]
fn reproducer_c_fn_hyn5make_called_rejected() {
    let src = r#"
fn hy_n5_make() int {
    return 9
}

fn main() {
    s := make([]int, 3)
    s[0] = 1
    fmt.println("{hy_n5_make()} {s[0]}")
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(error_messages(&r).iter().any(|m| m.contains("named `hy_n5_make`")));
}

#[test]
fn reproducer_d_const_set_with_set_use_rejected() {
    let src = r#"
const set = 1

fn main() {
    seen := set[int]{}
    seen.insert(3)
    fmt.println("{set} {seen.len()}")
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(error_messages(&r).iter().any(|m| m.contains("named `set`")));
}

#[test]
fn reproducer_e_local_hypop_shadow_rejected() {
    let src = r#"
fn main() {
    hy_pop := 1
    stack := []int{1, 2, 3}
    popped := stack.pop()
    fmt.println("{hy_pop} {popped}")
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(error_messages(&r).iter().any(|m| m.contains("named `hy_pop`")));
}

#[test]
fn reproducer_f_fn_hyn5make_declared_never_called_rejected() {
    let src = r#"
fn hy_n5_make() int {
    return 9
}

fn main() {
    s := make([]int, 3)
    s[0] = 1
    fmt.println("{s[0]}")
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(error_messages(&r).iter().any(|m| m.contains("named `hy_n5_make`")));
}

#[test]
fn user_fn_set_without_set_prelude_stays_accepted() {
    let src = r#"
fn set(dst []map[string]int, v map[string]int) {
    dst[0] = v
}

fn main() {
    var m map[string]int
    var xs []map[string]int
    xs = append(xs, m)
    set(xs, m)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected accept: {:?}", r.diagnostics);
}
