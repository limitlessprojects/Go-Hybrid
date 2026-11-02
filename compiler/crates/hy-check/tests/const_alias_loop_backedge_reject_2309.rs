//! Regression tests for HYB-E3006 (T22) loop back-edge rejections (#2309).
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
