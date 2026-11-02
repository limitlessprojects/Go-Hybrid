//! Regression tests for HYB-E3006 (T22) loop back-edge controls and positive acceptance (#2309).
use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

const PRELUDE: &str = "package main\n\nconst tags = map[string]int{\"a\": 1}\n\nfn reader(done chan int) {\n    s := 0\n    for i in 0..2000 {\n        s = s + tags[\"a\"]\n    }\n    _ = s\n    done <- 1\n}\n";

fn check(body: &str) -> hy_check::CheckResult {
    let src = format!("{}{}", PRELUDE, body);
    let ast = parse_file("inline.hy", &src)
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
fn a1_loop_reset_then_alias_matches_unrolled() {
    let looped = check("fn main() {\n    done := make(chan int, 1)\n    task reader(done)\n    t := map[string]int{}\n    for i in 0..2000 {\n        t = map[string]int{}\n        t[\"a\"] = i\n        t = tags\n    }\n    <-done\n}\n");
    assert_eq!(e3006_lines(&looped), vec![18, 19, 20]);
    assert_eq!(looped.verdict, Verdict::Reject);
    let unrolled = check("fn main() {\n    done := make(chan int, 1)\n    task reader(done)\n    t := map[string]int{}\n    t = map[string]int{}\n    t[\"a\"] = 0\n    t = tags\n    t = map[string]int{}\n    t[\"a\"] = 1\n    t = tags\n    <-done\n}\n");
    assert!(!e3006_lines(&unrolled).is_empty());
    assert_eq!(unrolled.verdict, Verdict::Reject);
}

#[test]
fn g1_to_g5_loop_reading_const_and_post_loop_alias_are_accepted() {
    assert_accepted("fn main() {\n    done := make(chan int, 1)\n    task reader(done)\n    s := 0\n    for i in 0..2000 {\n        s = s + tags[\"a\"] + i\n    }\n    _ = s\n    <-done\n}\n");
    assert_accepted("fn main() {\n    done := make(chan int, 1)\n    task reader(done)\n    t := map[string]int{}\n    s := 0\n    for i in 0..2000 {\n        u := tags\n        s = s + u[\"a\"]\n        t[\"a\"] = i\n    }\n    _ = s\n    <-done\n}\n");
    assert_accepted("fn touch(c map[string]int, v int) int {\n    t := map[string]int{}\n    s := 0\n    for i in 0..2 {\n        t[\"a\"] = v + i\n        s = s + c[\"a\"]\n    }\n    return s\n}\nfn main() {\n    done := make(chan int, 1)\n    task reader(done)\n    for i in 0..2000 {\n        _ = touch(tags, i)\n    }\n    <-done\n}\n");
    assert_accepted("fn main() {\n    done := make(chan int, 1)\n    task reader(done)\n    t := map[string]int{}\n    n := 0\n    for n < 2000 {\n        t[\"a\"] = n\n        n = n + 1\n    }\n    t = tags\n    _ = t[\"a\"]\n    <-done\n}\n");
    assert_accepted("fn main() {\n    done := make(chan int, 1)\n    task reader(done)\n    f := fn() {\n        t := map[string]int{}\n        for i in 0..2000 {\n            t[\"a\"] = i\n            _ = tags[\"a\"]\n        }\n    }\n    f()\n    <-done\n}\n");
}
