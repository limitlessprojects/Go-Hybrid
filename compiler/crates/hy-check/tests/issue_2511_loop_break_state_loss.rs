use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn run(src: &str) -> hy_check::CheckResult {
    check_file(&parse_file("test.hy", src).expect("snippet parses"))
}

fn check_err(src: &str, code: &str) {
    let r = run(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected reject, got: {:?}", r.diagnostics);
    assert!(r.diagnostics.iter().any(|d| d.code == code), "expected {code}, got: {:?}", r.diagnostics);
}

const WM: &str = "fn wm(x map[string]int, done chan int) {\n    done <- 1\n}\n";

#[test]
fn break_exit_keeps_non_owned_origin_e3004() {
    let src = format!("{WM}fn spawn2(x map[string]int, done chan int) {{\n    cur := map[string]int{{}}\n    for j in 0..2 {{\n        cur = x\n        if j == 1 {{\n            break\n        }}\n        cur = map[string]int{{}}\n    }}\n    task wm(move cur, done)\n}}\n");
    check_err(&src, "HYB-E3004");
}

#[test]
fn break_exit_keeps_alias_for_task_move_freeze_e3003() {
    let src = format!("{WM}fn main() {{\n    m := map[string]int{{\"a\": 1}}\n    a := map[string]int{{}}\n    done := make(chan int, 1)\n    for j in 0..2 {{\n        a = m\n        if j == 1 {{\n            break\n        }}\n        a = map[string]int{{}}\n    }}\n    task wm(move m, done)\n    a[\"a\"] = 1\n    <-done\n}}\n");
    check_err(&src, "HYB-E3003");
}

#[test]
fn break_exit_keeps_alias_for_send_move_freeze_e3003() {
    let src = "fn main() {\n    m := map[string]int{\"a\": 1}\n    a := map[string]int{}\n    ch := make(chan map[string]int, 1)\n    for j in 0..2 {\n        a = m\n        if j == 1 {\n            break\n        }\n        a = map[string]int{}\n    }\n    ch <- move m\n    a[\"a\"] = 1\n}\n";
    check_err(src, "HYB-E3003");
}

#[test]
fn break_exit_const_param_alias_write_e3006() {
    let src = "const tags = map[string]int{\"a\": 1}\nfn touch(c map[string]int, v int) {\n    t := map[string]int{}\n    for i in 0..2 {\n        t = c\n        if i >= 0 {\n            break\n        }\n        t = map[string]int{}\n    }\n    t[\"a\"] = v\n}\nfn main() {\n    touch(tags, 1)\n}\n";
    check_err(src, "HYB-E3006");
}

#[test]
fn break_exit_param_store_summary_e3003() {
    let src = "fn link(dst []map[string]int, src map[string]int, other map[string]int) {\n    tmp := other\n    for i in 0..2 {\n        tmp = src\n        if i >= 0 {\n            break\n        }\n        tmp = other\n    }\n    dst[0] = tmp\n}\nfn worker(s []map[string]int, done chan int) {\n    done <- 1\n}\nfn main() {\n    m := map[string]int{\"a\": 1}\n    o := map[string]int{\"b\": 1}\n    dst := []map[string]int{{}}\n    link(dst, m, o)\n    done := make(chan int, 1)\n    task worker(move dst, done)\n    m[\"a\"] = 2\n    <-done\n}\n";
    check_err(src, "HYB-E3003");
}

#[test]
fn closure_break_exit_keeps_non_owned_origin_e3004() {
    let src = format!("{WM}fn main() {{\n    f := fn(x map[string]int, done chan int) {{\n        cur := map[string]int{{}}\n        for j in 0..2 {{\n            cur = x\n            if j == 1 {{\n                break\n            }}\n            cur = map[string]int{{}}\n        }}\n        task wm(move cur, done)\n    }}\n    f(map[string]int{{}}, make(chan int, 1))\n}}\n");
    check_err(&src, "HYB-E3004");
}

#[test]
fn closure_break_exit_const_alias_write_e3006() {
    let src = "const tags = map[string]int{\"a\": 1}\nfn main() {\n    f := fn() {\n        t := map[string]int{}\n        for i in 0..2 {\n            t = tags\n            if i >= 0 {\n                break\n            }\n            t = map[string]int{}\n        }\n        t[\"a\"] = 1\n    }\n    f()\n}\n";
    check_err(src, "HYB-E3006");
}

#[test]
fn loop_break_with_owned_value_stays_accepted() {
    let src = format!("{WM}fn main() {{\n    cur := map[string]int{{\"k\": 1}}\n    done := make(chan int, 1)\n    for i in 0..2 {{\n        if i == 1 {{\n            break\n        }}\n    }}\n    task wm(move cur, done)\n    <-done\n}}\n");
    let r = run(&src);
    assert_eq!(r.verdict, Verdict::Accept, "expected accept, got: {:?}", r.diagnostics);
}

#[test]
fn infinite_loop_break_exit_keeps_non_owned_origin_e3004() {
    let src = format!("{WM}fn spawn2(x map[string]int, done chan int) {{\n    cur := map[string]int{{}}\n    n := 0\n    loop {{\n        cur = x\n        n = n + 1\n        if n == 2 {{\n            break\n        }}\n        cur = map[string]int{{}}\n    }}\n    task wm(move cur, done)\n}}\n");
    check_err(&src, "HYB-E3004");
}

#[test]
fn break_loop_from_inner_for_keeps_non_owned_origin_e3004() {
    let src = format!("{WM}fn spawn2(x map[string]int, done chan int) {{\n    cur := map[string]int{{}}\n    loop {{\n        for j in 0..2 {{\n            cur = x\n            break loop\n        }}\n        cur = map[string]int{{}}\n    }}\n    task wm(move cur, done)\n}}\n");
    check_err(&src, "HYB-E3004");
}

#[test]
fn select_break_does_not_leave_the_loop() {
    let src = format!("{WM}fn spawn2(x map[string]int, ch chan int, done chan int) {{\n    cur := map[string]int{{}}\n    for j in 0..2 {{\n        cur = x\n        select {{\n        v := <-ch:\n            break\n        }}\n        cur = map[string]int{{}}\n    }}\n    task wm(move cur, done)\n}}\n");
    let r = run(&src);
    assert_eq!(r.verdict, Verdict::Accept, "expected accept, got: {:?}", r.diagnostics);
}
