//! Regression tests for HYB-E3006 (T22) across loop back-edges (#2309).
use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;
const PRELUDE: &str = "package main\n\nconst tags = map[string]int{\"a\": 1}\n\nfn reader(done chan int) {\n    s := 0\n    for i in 0..2000 {\n        s = s + tags[\"a\"]\n    }\n    _ = s\n    done <- 1\n}\n";
fn check(body: &str) -> hy_check::CheckResult {
    let src = format!("{}{}", PRELUDE, body);
    let path = "inline.hy";
    let ast = parse_file(path, &src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}
fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}
fn e3006_lines(r: &hy_check::CheckResult) -> Vec<u32> {
    let mut v: Vec<u32> = r
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error && d.code == "HYB-E3006")
        .map(|d| d.span.start.line)
        .collect();
    v.sort_unstable();
    v.dedup();
    v
}
fn assert_e3006_at(body: &str, lines: &[u32]) {
    let r = check(body);
    let got = e3006_lines(&r);
    for l in lines {
        assert!(
            got.contains(l),
            "expected HYB-E3006 at line {}, got {:?} (all errors {:?})",
            l,
            got,
            codes(&r)
        );
    }
    assert_eq!(r.verdict, Verdict::Reject);
}
fn assert_accepted(body: &str) {
    let r = check(body);
    assert!(
        codes(&r).is_empty(),
        "expected no errors, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}
#[test]
fn k1_helper_for_carries_param_alias() {
    assert_e3006_at("fn touch(c map[string]int, v int) {\n    t := map[string]int{}\n    for i in 0..2 {\n        t[\"a\"] = v + i\n        t = c\n    }\n}\nfn main() {\n    done := make(chan int, 1)\n    task reader(done)\n    for i in 0..2000 {\n        touch(tags, i)\n    }\n    <-done\n}\n", &[24]);
}
#[test]
fn k2_helper_while_carries_param_alias() {
    assert_e3006_at("fn touch(c map[string]int, v int) {\n    t := map[string]int{}\n    n := 0\n    for n < 2 {\n        t[\"a\"] = v\n        t = c\n        n = n + 1\n    }\n}\nfn main() {\n    done := make(chan int, 1)\n    task reader(done)\n    for i in 0..2000 {\n        touch(tags, i)\n    }\n    <-done\n}\n", &[26]);
}
#[test]
fn k3_helper_loop_rebinds_to_const() {
    assert_e3006_at("fn touch(c map[string]int, v int) {\n    _ = c\n    t := map[string]int{}\n    for i in 0..2 {\n        t[\"a\"] = v + i\n        t = tags\n    }\n}\nfn main() {\n    done := make(chan int, 1)\n    task reader(done)\n    for i in 0..2000 {\n        touch(tags, i)\n    }\n    <-done\n}\n", &[17]);
}
#[test]
fn k4_main_loop_write_then_rebind() {
    assert_e3006_at("fn main() {\n    done := make(chan int, 1)\n    task reader(done)\n    t := map[string]int{}\n    for i in 0..2000 {\n        t[\"a\"] = i\n        t = tags\n    }\n    <-done\n}\n", &[18]);
}
#[test]
fn k5_closure_loop_write_then_rebind() {
    assert_e3006_at("fn main() {\n    done := make(chan int, 1)\n    task reader(done)\n    f := fn() {\n        t := map[string]int{}\n        for i in 0..2000 {\n            t[\"a\"] = i\n            t = tags\n        }\n    }\n    f()\n    <-done\n}\n", &[19]);
}
#[test]
fn k6_nested_inner_write_outer_rebind() {
    assert_e3006_at("fn main() {\n    done := make(chan int, 1)\n    task reader(done)\n    t := map[string]int{}\n    for j in 0..1000 {\n        for i in 0..2 {\n            t[\"a\"] = i + j\n        }\n        t = tags\n    }\n    <-done\n}\n", &[19]);
}
#[test]
fn k7_helper_nested_while_carries_param_alias() {
    assert_e3006_at("fn touch(c map[string]int, v int) {\n    t := map[string]int{}\n    n := 0\n    for n < 2 {\n        for i in 0..1 {\n            t[\"a\"] = v + i\n        }\n        t = c\n        n = n + 1\n    }\n}\nfn main() {\n    done := make(chan int, 1)\n    task reader(done)\n    for i in 0..2000 {\n        touch(tags, i)\n    }\n    <-done\n}\n", &[28]);
}
#[test]
fn k8_two_step_alias_chain_needs_fixpoint() {
    assert_e3006_at("fn main() {\n    done := make(chan int, 1)\n    task reader(done)\n    t := map[string]int{}\n    u := map[string]int{}\n    for i in 0..2000 {\n        t[\"a\"] = i\n        t = u\n        u = tags\n    }\n    <-done\n}\n", &[19]);
}
#[test]
fn k9_helper_two_step_alias_chain_needs_fixpoint() {
    assert_e3006_at("fn touch(c map[string]int, v int) {\n    t := map[string]int{}\n    u := map[string]int{}\n    for i in 0..3 {\n        t[\"a\"] = v + i\n        t = u\n        u = c\n    }\n}\nfn main() {\n    done := make(chan int, 1)\n    task reader(done)\n    for i in 0..2000 {\n        touch(tags, i)\n    }\n    <-done\n}\n", &[26]);
}
#[test]
fn c1_control_helper_direct_param_write() {
    assert_e3006_at("fn touch(c map[string]int, v int) {\n    c[\"a\"] = v\n}\nfn main() {\n    done := make(chan int, 1)\n    task reader(done)\n    for i in 0..2000 {\n        touch(tags, i)\n    }\n    <-done\n}\n", &[20]);
}
#[test]
fn c2_control_helper_straight_alias_write() {
    assert_e3006_at("fn touch(c map[string]int, v int) {\n    t := c\n    t[\"a\"] = v\n}\nfn main() {\n    done := make(chan int, 1)\n    task reader(done)\n    for i in 0..2000 {\n        touch(tags, i)\n    }\n    <-done\n}\n", &[21]);
}
#[test]
fn c3_control_loop_alias_then_write() {
    assert_e3006_at("fn main() {\n    done := make(chan int, 1)\n    task reader(done)\n    for i in 0..2000 {\n        t := tags\n        t[\"a\"] = i\n    }\n    <-done\n}\n", &[18]);
}
#[test]
fn x1_control_loop_rebind_then_write() {
    assert_e3006_at("fn main() {\n    done := make(chan int, 1)\n    task reader(done)\n    t := map[string]int{}\n    for i in 0..2000 {\n        t = tags\n        t[\"a\"] = i\n    }\n    <-done\n}\n", &[19]);
}
#[test]
fn x2_control_unrolled_write_rebind_write() {
    assert_e3006_at("fn main() {\n    done := make(chan int, 1)\n    task reader(done)\n    t := map[string]int{}\n    t[\"a\"] = 0\n    t = tags\n    t[\"a\"] = 1\n    t = tags\n    <-done\n}\n", &[19, 20]);
}
#[test]
fn a1_loop_reset_then_alias_matches_unrolled() {
    let looped = check("fn main() {\n    done := make(chan int, 1)\n    task reader(done)\n    t := map[string]int{}\n    for i in 0..2000 {\n        t = map[string]int{}\n        t[\"a\"] = i\n        t = tags\n    }\n    <-done\n}\n");
    assert_eq!(e3006_lines(&looped), vec![18, 19, 20]);
    assert_eq!(looped.verdict, Verdict::Reject);
    let unrolled = check("fn main() {\n    done := make(chan int, 1)\n    task reader(done)\n    t := map[string]int{}\n    t = map[string]int{}\n    t[\"a\"] = 0\n    t = tags\n    t = map[string]int{}\n    t[\"a\"] = 1\n    t = tags\n    <-done\n}\n");
    assert!(!e3006_lines(&unrolled).is_empty());
    assert_eq!(unrolled.verdict, Verdict::Reject);
}
#[test]
fn a3_loop_read_then_alias_matches_unrolled() {
    let looped = check("fn main() {\n    done := make(chan int, 1)\n    task reader(done)\n    t := map[string]int{}\n    s := 0\n    for i in 0..2000 {\n        s = s + t[\"a\"]\n        t = tags\n    }\n    _ = s\n    <-done\n}\n");
    assert_eq!(e3006_lines(&looped), vec![20]);
    assert_eq!(looped.verdict, Verdict::Reject);
    let unrolled = check("fn main() {\n    done := make(chan int, 1)\n    task reader(done)\n    t := map[string]int{}\n    s := 0\n    s = s + t[\"a\"]\n    t = tags\n    s = s + t[\"a\"]\n    t = tags\n    _ = s\n    <-done\n}\n");
    assert!(!e3006_lines(&unrolled).is_empty());
    assert_eq!(unrolled.verdict, Verdict::Reject);
}
#[test]
fn a4_loop_fresh_local_then_alias_matches_unrolled() {
    let looped = check("fn main() {\n    done := make(chan int, 1)\n    task reader(done)\n    t := map[string]int{}\n    for i in 0..2000 {\n        u := map[string]int{}\n        u[\"a\"] = i\n        t = tags\n        _ = t\n    }\n    <-done\n}\n");
    assert_eq!(e3006_lines(&looped), vec![20]);
    assert_eq!(looped.verdict, Verdict::Reject);
    let unrolled = check("fn main() {\n    done := make(chan int, 1)\n    task reader(done)\n    t := map[string]int{}\n    u := map[string]int{}\n    u[\"a\"] = 0\n    t = tags\n    _ = t\n    u2 := map[string]int{}\n    u2[\"a\"] = 1\n    t = tags\n    _ = t\n    <-done\n}\n");
    assert!(!e3006_lines(&unrolled).is_empty());
    assert_eq!(unrolled.verdict, Verdict::Reject);
}
#[test]
fn g1_loop_reading_const_is_accepted() {
    assert_accepted("fn main() {\n    done := make(chan int, 1)\n    task reader(done)\n    s := 0\n    for i in 0..2000 {\n        s = s + tags[\"a\"] + i\n    }\n    _ = s\n    <-done\n}\n");
}
#[test]
fn g2_loop_block_local_const_alias_is_accepted() {
    assert_accepted("fn main() {\n    done := make(chan int, 1)\n    task reader(done)\n    t := map[string]int{}\n    s := 0\n    for i in 0..2000 {\n        u := tags\n        s = s + u[\"a\"]\n        t[\"a\"] = i\n    }\n    _ = s\n    <-done\n}\n");
}
#[test]
fn g3_helper_loop_reading_param_is_accepted() {
    assert_accepted("fn touch(c map[string]int, v int) int {\n    t := map[string]int{}\n    s := 0\n    for i in 0..2 {\n        t[\"a\"] = v + i\n        s = s + c[\"a\"]\n    }\n    return s\n}\nfn main() {\n    done := make(chan int, 1)\n    task reader(done)\n    for i in 0..2000 {\n        _ = touch(tags, i)\n    }\n    <-done\n}\n");
}
#[test]
fn g4_alias_made_after_loop_is_accepted() {
    assert_accepted("fn main() {\n    done := make(chan int, 1)\n    task reader(done)\n    t := map[string]int{}\n    n := 0\n    for n < 2000 {\n        t[\"a\"] = n\n        n = n + 1\n    }\n    t = tags\n    _ = t[\"a\"]\n    <-done\n}\n");
}
#[test]
fn g5_closure_loop_without_alias_is_accepted() {
    assert_accepted("fn main() {\n    done := make(chan int, 1)\n    task reader(done)\n    f := fn() {\n        t := map[string]int{}\n        for i in 0..2000 {\n            t[\"a\"] = i\n            _ = tags[\"a\"]\n        }\n    }\n    f()\n    <-done\n}\n");
}
