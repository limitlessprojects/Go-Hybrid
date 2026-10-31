//! const_alias_loop_backedge_issue_2309.rs: regression tests for HYB-E3006
//! (T22) across loop back-edges, from soundness issue #2309. A binding that
//! becomes a `const` alias late in a loop body (`t["a"] = i; t = tags`) is
//! that alias at the top of the next iteration, so the earlier write lands
//! in the `const`. Main accepted these with 0 diagnostics and the emitted Go
//! raced (`go run -race` DATA RACE against a reader task). The checker now
//! replays the body along the back-edge until the set of `const` aliases
//! stops growing, in the main pass, in closure bodies, and in the
//! interprocedural helper summary.
//!
//! Precision: loop verdicts match the unrolled straight-line program. A
//! whole-binding rebind of a binding that is already a `const` alias is
//! HYB-E3006 in straight-line code (#123, `const_write_root`), so the a1/a3/a4
//! loops are rejected exactly where their unrolled u1/u3/u4 forms are.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

/// Shared prelude: a file-level `const` map and a reader task that reads it
/// concurrently, so any write that reaches `tags` races at runtime. It is
/// 12 lines, so each body starts on line 13.
const PRELUDE: &str = r#"package main

const tags = map[string]int{"a": 1}

fn reader(done chan int) {
    s := 0
    for i in 0..2000 {
        s = s + tags["a"]
    }
    _ = s
    done <- 1
}
"#;

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

/// Source lines carrying HYB-E3006, sorted and deduplicated.
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

// ---------------------------------------------------------------------------
// Rejects: a write before a loop-carried `const` alias (accepted on main)
// ---------------------------------------------------------------------------

#[test]
fn k1_helper_for_carries_param_alias() {
    // helper `t["a"] = v + i; t = c` in a for loop, called with `tags`: the call site is the write.
    assert_e3006_at(
        r#"fn touch(c map[string]int, v int) {
    t := map[string]int{}
    for i in 0..2 {
        t["a"] = v + i
        t = c
    }
}
fn main() {
    done := make(chan int, 1)
    task reader(done)
    for i in 0..2000 {
        touch(tags, i)
    }
    <-done
}
"#,
        &[24],
    );
}

#[test]
fn k2_helper_while_carries_param_alias() {
    // same as k1 with a `for n < 2` while loop.
    assert_e3006_at(
        r#"fn touch(c map[string]int, v int) {
    t := map[string]int{}
    n := 0
    for n < 2 {
        t["a"] = v
        t = c
        n = n + 1
    }
}
fn main() {
    done := make(chan int, 1)
    task reader(done)
    for i in 0..2000 {
        touch(tags, i)
    }
    <-done
}
"#,
        &[26],
    );
}

#[test]
fn k3_helper_loop_rebinds_to_const() {
    // helper loop rebinds the local to the file-level const directly.
    assert_e3006_at(
        r#"fn touch(c map[string]int, v int) {
    _ = c
    t := map[string]int{}
    for i in 0..2 {
        t["a"] = v + i
        t = tags
    }
}
fn main() {
    done := make(chan int, 1)
    task reader(done)
    for i in 0..2000 {
        touch(tags, i)
    }
    <-done
}
"#,
        &[17],
    );
}

#[test]
fn k4_main_loop_write_then_rebind() {
    // issue #2309's main-pass shape.
    assert_e3006_at(
        r#"fn main() {
    done := make(chan int, 1)
    task reader(done)
    t := map[string]int{}
    for i in 0..2000 {
        t["a"] = i
        t = tags
    }
    <-done
}
"#,
        &[18],
    );
}

#[test]
fn k5_closure_loop_write_then_rebind() {
    // the k4 loop inside a closure (scan_block_const_writes path).
    assert_e3006_at(
        r#"fn main() {
    done := make(chan int, 1)
    task reader(done)
    f := fn() {
        t := map[string]int{}
        for i in 0..2000 {
            t["a"] = i
            t = tags
        }
    }
    f()
    <-done
}
"#,
        &[19],
    );
}

#[test]
fn k6_nested_inner_write_outer_rebind() {
    // write in an inner loop, alias made at the end of the outer body.
    assert_e3006_at(
        r#"fn main() {
    done := make(chan int, 1)
    task reader(done)
    t := map[string]int{}
    for j in 0..1000 {
        for i in 0..2 {
            t["a"] = i + j
        }
        t = tags
    }
    <-done
}
"#,
        &[19],
    );
}

#[test]
fn k7_helper_nested_while_carries_param_alias() {
    // helper: inner for write, outer while rebind to the param.
    assert_e3006_at(
        r#"fn touch(c map[string]int, v int) {
    t := map[string]int{}
    n := 0
    for n < 2 {
        for i in 0..1 {
            t["a"] = v + i
        }
        t = c
        n = n + 1
    }
}
fn main() {
    done := make(chan int, 1)
    task reader(done)
    for i in 0..2000 {
        touch(tags, i)
    }
    <-done
}
"#,
        &[28],
    );
}

#[test]
fn k8_two_step_alias_chain_needs_fixpoint() {
    // `t = u; u = tags`: t reaches the const only on the third iteration, so one replay is not enough.
    assert_e3006_at(
        r#"fn main() {
    done := make(chan int, 1)
    task reader(done)
    t := map[string]int{}
    u := map[string]int{}
    for i in 0..2000 {
        t["a"] = i
        t = u
        u = tags
    }
    <-done
}
"#,
        &[19],
    );
}

#[test]
fn k9_helper_two_step_alias_chain_needs_fixpoint() {
    // k8 through a helper parameter.
    assert_e3006_at(
        r#"fn touch(c map[string]int, v int) {
    t := map[string]int{}
    u := map[string]int{}
    for i in 0..3 {
        t["a"] = v + i
        t = u
        u = c
    }
}
fn main() {
    done := make(chan int, 1)
    task reader(done)
    for i in 0..2000 {
        touch(tags, i)
    }
    <-done
}
"#,
        &[26],
    );
}

// ---------------------------------------------------------------------------
// Controls: already HYB-E3006 on main, must stay rejected
// ---------------------------------------------------------------------------

#[test]
fn c1_control_helper_direct_param_write() {
    // control: already rejected on main.
    assert_e3006_at(
        r#"fn touch(c map[string]int, v int) {
    c["a"] = v
}
fn main() {
    done := make(chan int, 1)
    task reader(done)
    for i in 0..2000 {
        touch(tags, i)
    }
    <-done
}
"#,
        &[20],
    );
}

#[test]
fn c2_control_helper_straight_alias_write() {
    // control: already rejected on main.
    assert_e3006_at(
        r#"fn touch(c map[string]int, v int) {
    t := c
    t["a"] = v
}
fn main() {
    done := make(chan int, 1)
    task reader(done)
    for i in 0..2000 {
        touch(tags, i)
    }
    <-done
}
"#,
        &[21],
    );
}

#[test]
fn c3_control_loop_alias_then_write() {
    // control: already rejected on main.
    assert_e3006_at(
        r#"fn main() {
    done := make(chan int, 1)
    task reader(done)
    for i in 0..2000 {
        t := tags
        t["a"] = i
    }
    <-done
}
"#,
        &[18],
    );
}

#[test]
fn x1_control_loop_rebind_then_write() {
    // control: already rejected on main.
    assert_e3006_at(
        r#"fn main() {
    done := make(chan int, 1)
    task reader(done)
    t := map[string]int{}
    for i in 0..2000 {
        t = tags
        t["a"] = i
    }
    <-done
}
"#,
        &[19],
    );
}

#[test]
fn x2_control_unrolled_write_rebind_write() {
    // control: already rejected on main.
    assert_e3006_at(
        r#"fn main() {
    done := make(chan int, 1)
    task reader(done)
    t := map[string]int{}
    t["a"] = 0
    t = tags
    t["a"] = 1
    t = tags
    <-done
}
"#,
        &[19, 20],
    );
}

// ---------------------------------------------------------------------------
// Consistency: rebinding an existing `const` alias is HYB-E3006 in
// straight-line code (#123), so the loop forms are rejected exactly where
// their unrolled forms are, no stricter
// ---------------------------------------------------------------------------

#[test]
fn a1_loop_reset_then_alias_matches_unrolled() {
    let looped = check(
        r#"fn main() {
    done := make(chan int, 1)
    task reader(done)
    t := map[string]int{}
    for i in 0..2000 {
        t = map[string]int{}
        t["a"] = i
        t = tags
    }
    <-done
}
"#,
    );
    assert_eq!(e3006_lines(&looped), vec![18, 19, 20]);
    assert_eq!(looped.verdict, Verdict::Reject);
    // The unrolled program is already rejected on main by the same rule.
    let unrolled = check(
        r#"fn main() {
    done := make(chan int, 1)
    task reader(done)
    t := map[string]int{}
    t = map[string]int{}
    t["a"] = 0
    t = tags
    t = map[string]int{}
    t["a"] = 1
    t = tags
    <-done
}
"#,
    );
    assert!(!e3006_lines(&unrolled).is_empty());
    assert_eq!(unrolled.verdict, Verdict::Reject);
}

#[test]
fn a3_loop_read_then_alias_matches_unrolled() {
    let looped = check(
        r#"fn main() {
    done := make(chan int, 1)
    task reader(done)
    t := map[string]int{}
    s := 0
    for i in 0..2000 {
        s = s + t["a"]
        t = tags
    }
    _ = s
    <-done
}
"#,
    );
    assert_eq!(e3006_lines(&looped), vec![20]);
    assert_eq!(looped.verdict, Verdict::Reject);
    // The unrolled program is already rejected on main by the same rule.
    let unrolled = check(
        r#"fn main() {
    done := make(chan int, 1)
    task reader(done)
    t := map[string]int{}
    s := 0
    s = s + t["a"]
    t = tags
    s = s + t["a"]
    t = tags
    _ = s
    <-done
}
"#,
    );
    assert!(!e3006_lines(&unrolled).is_empty());
    assert_eq!(unrolled.verdict, Verdict::Reject);
}

#[test]
fn a4_loop_fresh_local_then_alias_matches_unrolled() {
    let looped = check(
        r#"fn main() {
    done := make(chan int, 1)
    task reader(done)
    t := map[string]int{}
    for i in 0..2000 {
        u := map[string]int{}
        u["a"] = i
        t = tags
        _ = t
    }
    <-done
}
"#,
    );
    assert_eq!(e3006_lines(&looped), vec![20]);
    assert_eq!(looped.verdict, Verdict::Reject);
    // The unrolled program is already rejected on main by the same rule.
    let unrolled = check(
        r#"fn main() {
    done := make(chan int, 1)
    task reader(done)
    t := map[string]int{}
    u := map[string]int{}
    u["a"] = 0
    t = tags
    _ = t
    u2 := map[string]int{}
    u2["a"] = 1
    t = tags
    _ = t
    <-done
}
"#,
    );
    assert!(!e3006_lines(&unrolled).is_empty());
    assert_eq!(unrolled.verdict, Verdict::Reject);
}

// ---------------------------------------------------------------------------
// Accepts: loops that never write through a `const` alias
// ---------------------------------------------------------------------------

#[test]
fn g1_loop_reading_const_is_accepted() {
    assert_accepted(
        r#"fn main() {
    done := make(chan int, 1)
    task reader(done)
    s := 0
    for i in 0..2000 {
        s = s + tags["a"] + i
    }
    _ = s
    <-done
}
"#,
    );
}

#[test]
fn g2_loop_block_local_const_alias_is_accepted() {
    assert_accepted(
        r#"fn main() {
    done := make(chan int, 1)
    task reader(done)
    t := map[string]int{}
    s := 0
    for i in 0..2000 {
        u := tags
        s = s + u["a"]
        t["a"] = i
    }
    _ = s
    <-done
}
"#,
    );
}

#[test]
fn g3_helper_loop_reading_param_is_accepted() {
    assert_accepted(
        r#"fn touch(c map[string]int, v int) int {
    t := map[string]int{}
    s := 0
    for i in 0..2 {
        t["a"] = v + i
        s = s + c["a"]
    }
    return s
}
fn main() {
    done := make(chan int, 1)
    task reader(done)
    for i in 0..2000 {
        _ = touch(tags, i)
    }
    <-done
}
"#,
    );
}

#[test]
fn g4_alias_made_after_loop_is_accepted() {
    assert_accepted(
        r#"fn main() {
    done := make(chan int, 1)
    task reader(done)
    t := map[string]int{}
    n := 0
    for n < 2000 {
        t["a"] = n
        n = n + 1
    }
    t = tags
    _ = t["a"]
    <-done
}
"#,
    );
}

#[test]
fn g5_closure_loop_without_alias_is_accepted() {
    assert_accepted(
        r#"fn main() {
    done := make(chan int, 1)
    task reader(done)
    f := fn() {
        t := map[string]int{}
        for i in 0..2000 {
            t["a"] = i
            _ = tags["a"]
        }
    }
    f()
    <-done
}
"#,
    );
}
