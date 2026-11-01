use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test must parse");
    check_file(&ast)
}

#[test]
fn t22_const_mutation_via_copy_into_composite_literal_field_is_rejected() {
    let src = "
struct holder { s []map[string]int }
fn sneak_mutate(m map[string]int) {
    dst := []map[string]int{map[string]int{}}
    src := []map[string]int{m}
    copy((holder{s: dst}).s, src)
    dst[0][\"k\"] = 99
}
fn main() {
    const cm = map[string]int{\"k\": 1}
    sneak_mutate(cm)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, got {:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006 on const mutation through copy into composite literal field, got {:?}",
        r.diagnostics
    );
}

#[test]
fn t22_const_mutation_via_helper_store_into_composite_literal_field_is_rejected() {
    let src = "
struct holder { s []map[string]int }
fn put(dst []map[string]int, src map[string]int) {
    dst[0] = src
}
fn sneak_mutate_via_fn(m map[string]int) {
    dst := []map[string]int{map[string]int{}}
    put((holder{s: dst}).s, m)
    dst[0][\"k\"] = 99
}
fn main() {
    const cm = map[string]int{\"k\": 1}
    sneak_mutate_via_fn(cm)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, got {:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006 on const mutation through helper store into composite literal field, got {:?}",
        r.diagnostics
    );
}

#[test]
fn o52_interprocedural_local_relay_via_composite_literal_field_freezes_caller_alias() {
    let src = "
struct holder { s []map[string]int }
fn relay(out []map[string]int, src []map[string]int) {
    tmp := []map[string]int{map[string]int{}}
    copy((holder{s: tmp}).s, src)
    copy(out, tmp)
}
fn sink(x []map[string]int) {}
fn main() {
    a := []map[string]int{map[string]int{\"k\": 1}}
    b := []map[string]int{map[string]int{}}
    relay(b, a)
    task sink(move a)
    b[0][\"k\"] = 2
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, got {:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 on mutating `b` after `a` was moved into a task across a local relay copy, got {:?}",
        r.diagnostics
    );
}
