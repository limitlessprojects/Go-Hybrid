//! Regression tests for issue #2180:
//! Non-ident destination arguments in copy and helper functions must not bypass
//! T22 const-alias tracking (HYB-E3006) or O5.2 interprocedural param-store tracking (HYB-E3003).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("issue_2180.hy", src).expect("test snippet must parse");
    check_file(&ast)
}

#[test]
fn test_issue_2180_copy_non_ident_dst_rejects_e3006() {
    let src = r#"
struct holder { s []map[string]int }
fn sneak_mutate(m map[string]int) {
    dst := []map[string]int{map[string]int{}}
    src := []map[string]int{m}
    copy((holder{s: dst}).s, src)
    dst[0]["k"] = 99
}
fn main() {
    const cm = map[string]int{"k": 1}
    sneak_mutate(cm)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected rejection, got {:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_issue_2180_callee_non_ident_dst_rejects_e3006() {
    let src = r#"
struct holder { s []map[string]int }
fn put(dst []map[string]int, src map[string]int) {
    dst[0] = src
}
fn sneak_mutate_via_fn(m map[string]int) {
    dst := []map[string]int{map[string]int{}}
    put((holder{s: dst}).s, m)
    dst[0]["k"] = 99
}
fn main() {
    const cm = map[string]int{"k": 1}
    sneak_mutate_via_fn(cm)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected rejection, got {:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006, got {:?}",
        r.diagnostics
    );
}

#[test]
fn test_issue_2180_relay_non_ident_dst_rejects_e3003() {
    let src = r#"
struct holder { s []map[string]int }
fn relay(out []map[string]int, src []map[string]int) {
    tmp := []map[string]int{map[string]int{}}
    copy((holder{s: tmp}).s, src)
    copy(out, tmp)
}
fn sink(x []map[string]int) {}
fn main() {
    a := []map[string]int{map[string]int{"k": 1}}
    b := []map[string]int{map[string]int{}}
    relay(b, a)
    task sink(move a)
    b[0]["k"] = 2
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected rejection, got {:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003, got {:?}",
        r.diagnostics
    );
}
