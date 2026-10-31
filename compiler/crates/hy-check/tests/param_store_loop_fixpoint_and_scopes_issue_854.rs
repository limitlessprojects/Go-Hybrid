//! Regression tests for issue #854 and the loop-carried helper store gap:
//! `walk_stmt_param_stores` (the interprocedural `(dst, src)` summary behind
//! HYB-E3003, O5.2) walked a loop body once, so an alias assigned in one
//! iteration and stored in the next was never linked; it also dropped aliases
//! assigned inside a loop, match or select arm to a local that had no alias
//! entry before it, and let an inner block's shadowing define erase the outer
//! name. Each reject below was `hyb check` OK on 1f0ff97 and races under
//! `go run -race`. The accepts guard against over-joining.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let file = parse_file("test.hy", src).expect("parse should succeed");
    check_file(&file)
}

fn assert_e3003(src: &str, why: &str) {
    let res = check(src);
    assert_eq!(
        res.verdict,
        Verdict::Reject,
        "{why}: expected Reject, got: {:?}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "{why}: expected HYB-E3003, got: {:?}",
        res.diagnostics
    );
}

fn assert_accepted(src: &str, why: &str) {
    let res = check(src);
    assert!(
        res.diagnostics.is_empty(),
        "{why}: expected no diagnostics, got: {:?}",
        res.diagnostics
    );
}

#[test]
fn loop_carried_store_in_range_loop_is_e3003() {
    assert_e3003(
        r#"
fn link(dst []map[string]int, src map[string]int, other map[string]int) {
    tmp := other
    for i in 0..2 {
        dst[0] = tmp
        tmp = src
    }
}
fn worker(s []map[string]int, done chan int) {
    s[0]["race"] = 1
    done <- 1
}
fn main() {
    m := map[string]int{"a": 1}
    o := map[string]int{"b": 1}
    dst := []map[string]int{{}}
    link(dst, m, o)
    done := make(chan int, 1)
    task worker(move dst, done)
    m["a"] = 2
    <-done
}
"#,
        "iteration 1 stores `other`, iteration 2 stores `src`; the second pass must link (dst, src)",
    );
}

#[test]
fn loop_carried_store_in_while_loop_is_e3003() {
    assert_e3003(
        r#"
fn link(dst []map[string]int, src map[string]int, other map[string]int) {
    tmp := other
    i := 0
    for i < 2 {
        dst[0] = tmp
        tmp = src
        i = i + 1
    }
}
fn worker(s []map[string]int, done chan int) {
    s[0]["race"] = 1
    done <- 1
}
fn main() {
    m := map[string]int{"a": 1}
    o := map[string]int{"b": 1}
    dst := []map[string]int{{}}
    link(dst, m, o)
    done := make(chan int, 1)
    task worker(move dst, done)
    m["a"] = 2
    <-done
}
"#,
        "same chain through a `for cond` loop",
    );
}

#[test]
fn loop_carried_store_from_fresh_literal_is_e3003() {
    assert_e3003(
        r#"
fn link(dst []map[string]int, src map[string]int) {
    tmp := map[string]int{}
    for i in 0..2 {
        dst[0] = tmp
        tmp = src
    }
}
fn worker(s []map[string]int, done chan int) {
    s[0]["race"] = 1
    done <- 1
}
fn main() {
    m := map[string]int{"a": 1}
    dst := []map[string]int{{}}
    link(dst, m)
    done := make(chan int, 1)
    task worker(move dst, done)
    m["a"] = 2
    <-done
}
"#,
        "`tmp` starts as a fresh literal, so it has no alias entry before the loop",
    );
}

#[test]
fn loop_carried_three_hop_chain_is_e3003() {
    assert_e3003(
        r#"
fn link(dst []map[string]int, src map[string]int, other map[string]int) {
    a := other
    b := other
    c := other
    for i in 0..4 {
        dst[0] = c
        c = b
        b = a
        a = src
    }
}
fn worker(s []map[string]int, done chan int) {
    s[0]["race"] = 1
    done <- 1
}
fn main() {
    m := map[string]int{"a": 1}
    o := map[string]int{"b": 1}
    dst := []map[string]int{{}}
    link(dst, m, o)
    done := make(chan int, 1)
    task worker(move dst, done)
    m["a"] = 2
    <-done
}
"#,
        "needs three passes to reach the fixpoint",
    );
}

#[test]
fn loop_carried_through_nested_loops_is_e3003() {
    assert_e3003(
        r#"
fn link(dst []map[string]int, src map[string]int, other map[string]int) {
    a := other
    b := other
    for i in 0..2 {
        for j in 0..2 {
            dst[0] = b
            b = a
        }
        a = src
    }
}
fn worker(s []map[string]int, done chan int) {
    s[0]["race"] = 1
    done <- 1
}
fn main() {
    m := map[string]int{"a": 1}
    o := map[string]int{"b": 1}
    dst := []map[string]int{{}}
    link(dst, m, o)
    done := make(chan int, 1)
    task worker(move dst, done)
    m["a"] = 2
    <-done
}
"#,
        "outer loop carries `a = src` into the inner loop chain",
    );
}

#[test]
fn predeclared_var_assigned_in_for_is_e3003_issue_854() {
    assert_e3003(
        r#"
fn link_via_for(dst []map[string]int, src map[string]int) {
    var tmp map[string]int
    for i in 0..1 {
        tmp = src
    }
    dst[0] = tmp
}
fn worker(s []map[string]int, done chan int) {
    s[0]["race"] = 1
    done <- 1
}
fn main() {
    m := map[string]int{"a": 1}
    dst := []map[string]int{{}}
    link_via_for(dst, m)
    done := make(chan int, 1)
    task worker(move dst, done)
    m["a"] = 2
    <-done
}
"#,
        "issue #854 reproducer: `var tmp` has no alias entry before the loop",
    );
}

#[test]
fn predeclared_var_assigned_in_match_arm_is_e3003_issue_854() {
    assert_e3003(
        r#"
fn link(dst []map[string]int, src map[string]int, o option[int]) {
    var tmp map[string]int
    match o {
        some(v) => {
            tmp = src
        }
        none => {}
    }
    dst[0] = tmp
}
fn worker(s []map[string]int, done chan int) {
    s[0]["race"] = 1
    done <- 1
}
fn main() {
    m := map[string]int{"a": 1}
    dst := []map[string]int{{}}
    link(dst, m, some(1))
    done := make(chan int, 1)
    task worker(move dst, done)
    m["a"] = 2
    <-done
}
"#,
        "issue #854 case 1, match arm",
    );
}

#[test]
fn predeclared_var_assigned_in_select_arm_is_e3003_issue_854() {
    assert_e3003(
        r#"
fn link(dst []map[string]int, src map[string]int, c chan int) {
    var tmp map[string]int
    select {
        <-c: {
            tmp = src
        }
        default: {
        }
    }
    dst[0] = tmp
}
fn worker(s []map[string]int, done chan int) {
    s[0]["race"] = 1
    done <- 1
}
fn main() {
    m := map[string]int{"a": 1}
    dst := []map[string]int{{}}
    c := make(chan int, 1)
    c <- 1
    link(dst, m, c)
    done := make(chan int, 1)
    task worker(move dst, done)
    m["a"] = 2
    <-done
}
"#,
        "issue #854 case 1, select arm",
    );
}

#[test]
fn inner_block_shadow_keeps_outer_param_alias_is_e3003_issue_854() {
    assert_e3003(
        r#"
fn link(dst []map[string]int, src map[string]int) {
    {
        src := map[string]int{}
        src["x"] = 1
    }
    dst[0] = src
}
fn worker(s []map[string]int, done chan int) {
    s[0]["race"] = 1
    done <- 1
}
fn main() {
    m := map[string]int{"a": 1}
    dst := []map[string]int{{}}
    link(dst, m)
    done := make(chan int, 1)
    task worker(move dst, done)
    m["a"] = 2
    <-done
}
"#,
        "issue #854 case 2: `{ src := .. }` must not erase the outer `src`",
    );
}

#[test]
fn assignment_before_block_shadow_is_kept_is_e3003() {
    assert_e3003(
        r#"
fn link(dst []map[string]int, src map[string]int, other map[string]int) {
    tmp := map[string]int{}
    {
        tmp = src
        tmp := other
        tmp["x"] = 1
    }
    dst[0] = tmp
}
fn worker(s []map[string]int, done chan int) {
    s[0]["race"] = 1
    done <- 1
}
fn main() {
    m := map[string]int{"a": 1}
    o := map[string]int{"b": 1}
    dst := []map[string]int{{}}
    link(dst, m, o)
    done := make(chan int, 1)
    task worker(move dst, done)
    m["a"] = 2
    <-done
}
"#,
        "`tmp = src` precedes the shadowing `tmp := other` in the same block",
    );
}

#[test]
fn straight_line_store_control_still_e3003() {
    assert_e3003(
        r#"
fn link(dst []map[string]int, src map[string]int, other map[string]int) {
    tmp := other
    tmp = src
    dst[0] = tmp
}
fn worker(s []map[string]int, done chan int) {
    s[0]["race"] = 1
    done <- 1
}
fn main() {
    m := map[string]int{"a": 1}
    o := map[string]int{"b": 1}
    dst := []map[string]int{{}}
    link(dst, m, o)
    done := make(chan int, 1)
    task worker(move dst, done)
    m["a"] = 2
    <-done
}
"#,
        "control: no loop in the helper",
    );
}

#[test]
fn loop_carried_store_inline_in_caller_still_e3003() {
    assert_e3003(
        r#"
fn worker(s []map[string]int, done chan int) {
    s[0]["race"] = 1
    done <- 1
}
fn main() {
    m := map[string]int{"a": 1}
    o := map[string]int{"b": 1}
    dst := []map[string]int{{}}
    tmp := o
    for i in 0..2 {
        dst[0] = tmp
        tmp = m
    }
    done := make(chan int, 1)
    task worker(move dst, done)
    m["a"] = 2
    <-done
}
"#,
        "control: the same loop inline in main",
    );
}

#[test]
fn fresh_copy_per_iteration_is_accepted() {
    assert_accepted(
        r#"
fn fill(dst []map[string]int, src map[string]int) {
    for i in 0..3 {
        tmp := map[string]int{}
        for k, v in src {
            tmp[k] = v + i
        }
        dst[i] = tmp
    }
}
fn worker(s []map[string]int, done chan int) {
    s[0]["race"] = 1
    done <- 1
}
fn main() {
    m := map[string]int{"a": 1}
    dst := []map[string]int{{}, {}, {}}
    fill(dst, m)
    done := make(chan int, 1)
    task worker(move dst, done)
    m["a"] = 2
    <-done
}
"#,
        "neg_loop_fresh",
    );
}

#[test]
fn loop_carried_fresh_values_are_accepted() {
    assert_accepted(
        r#"
fn link(dst []map[string]int, src map[string]int) {
    tmp := map[string]int{}
    i := 0
    for i < 3 {
        dst[0] = tmp
        tmp = map[string]int{"n": len(src) + i}
        i = i + 1
    }
}
fn worker(s []map[string]int, done chan int) {
    s[0]["race"] = 1
    done <- 1
}
fn main() {
    m := map[string]int{"a": 1}
    dst := []map[string]int{{}}
    link(dst, m)
    done := make(chan int, 1)
    task worker(move dst, done)
    m["a"] = 2
    <-done
}
"#,
        "neg_loop_carried_fresh",
    );
}

#[test]
fn inner_block_shadow_does_not_leak_out_is_accepted() {
    assert_accepted(
        r#"
fn link(dst []map[string]int, src map[string]int) {
    tmp := map[string]int{}
    {
        tmp := src
        tmp["seen"] = 1
    }
    dst[0] = tmp
}
fn worker(s []map[string]int, done chan int) {
    s[0]["race"] = 1
    done <- 1
}
fn main() {
    m := map[string]int{"a": 1}
    dst := []map[string]int{{}}
    link(dst, m)
    done := make(chan int, 1)
    task worker(move dst, done)
    m["a"] = 2
    <-done
}
"#,
        "neg_block_shadow",
    );
}

#[test]
fn loop_body_shadow_does_not_leak_out_is_accepted() {
    assert_accepted(
        r#"
fn link(dst []map[string]int, src map[string]int, other map[string]int) {
    tmp := map[string]int{}
    for i in 0..2 {
        tmp := src
        tmp["x"] = i
    }
    dst[0] = tmp
    other["y"] = 1
}
fn worker(s []map[string]int, done chan int) {
    s[0]["race"] = 1
    done <- 1
}
fn main() {
    m := map[string]int{"a": 1}
    o := map[string]int{"b": 1}
    dst := []map[string]int{{}}
    link(dst, m, o)
    done := make(chan int, 1)
    task worker(move dst, done)
    m["a"] = 2
    <-done
}
"#,
        "neg_loop_shadow_body",
    );
}

#[test]
fn match_arm_binding_does_not_leak_out_is_accepted() {
    assert_accepted(
        r#"
fn link(dst []map[string]int, src map[string]int, o option[map[string]int]) {
    tmp := map[string]int{}
    match o {
        some(v) => {
            v["k"] = len(src)
        }
        none => {}
    }
    dst[0] = tmp
}
fn worker(s []map[string]int, done chan int) {
    s[0]["race"] = 1
    done <- 1
}
fn main() {
    m := map[string]int{"a": 1}
    dst := []map[string]int{{}}
    link(dst, m, some(map[string]int{}))
    done := make(chan int, 1)
    task worker(move dst, done)
    m["a"] = 2
    <-done
}
"#,
        "neg_match_binding",
    );
}

#[test]
fn nested_loops_over_values_are_accepted() {
    assert_accepted(
        r#"
fn total(rows [][]int) int {
    acc := 0
    for r in rows {
        for x in r {
            acc = acc + x
        }
    }
    return acc
}
fn sum_into(dst []int, rows [][]int) {
    for i in 0..len(dst) {
        dst[i] = total(rows) + i
    }
}
fn worker(s []int, done chan int) {
    s[0] = 1
    done <- 1
}
fn main() {
    rows := [][]int{{1, 2}, {3}}
    dst := []int{0, 0}
    sum_into(dst, rows)
    done := make(chan int, 1)
    task worker(move dst, done)
    rows[0][0] = 9
    <-done
}
"#,
        "neg_nested_loops",
    );
}
