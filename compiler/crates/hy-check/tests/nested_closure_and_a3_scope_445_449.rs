//! Integration tests for issues #445 and #449:
//! - #445: nested closures inside `else if` conditions, `if` / `else if`
//!   init-scoped conditions, `match` guards with pattern bindings, and `for`
//!   headers inside an enclosing closure body must not be dropped or lose
//!   their active header scope during the closure-boundary pass.
//! - #449: lexical capture snapshotting and post-closure move propagation for
//!   nested closures (same-block late local shadowing, child-block may-share
//!   peer moves, sub-block local moves) and A3 `HYB-E3102` map-slot loan
//!   invalidation across composite/call parameter rebinds, branch-assigned
//!   named results, and inner-block closure shadows.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src)
        .unwrap_or_else(|diags| panic!("test program failed to parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(res: &hy_check::CheckResult) -> Vec<String> {
    res.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

// ===========================================================================
// Issue #445 — else-if / header-scoped nested closures
// ===========================================================================

#[test]
fn else_if_cond_closure_spawning_captured_map_rejects_e3001_445() {
    let r = check(
        r#"
fn worker(m map[string]int, done chan int) {
    m["worker"] = 1
    done <- 1
}

fn run(done chan int) {
    m := map[string]int{"init": 0}
    outer := fn() {
        if false {
        } else if (fn() bool {
            task worker(m, done)
            return false
        })() {
        }
    }
    outer()
    m["main"] = 2
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, got {:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 for unmoved captured map spawn in else-if condition closure, got {:?}",
        r.diagnostics
    );
}

#[test]
fn else_if_cond_closure_sending_captured_map_rejects_e3002_445() {
    let r = check(
        r#"
fn run(ch chan map[string]int) {
    m := map[string]int{"init": 0}
    outer := fn() {
        if false {
        } else if false {
        } else if (fn() bool {
            ch <- m
            return false
        })() {
        }
    }
    outer()
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, got {:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E3002"),
        "expected HYB-E3002 for unmoved captured map send in chained else-if condition closure, got {:?}",
        r.diagnostics
    );
}

#[test]
fn if_and_else_if_init_scoped_cond_closure_rejects_e3001_445() {
    let r = check(
        r#"
fn worker(m map[string]int, done chan int) {
    m["worker"] = 1
    done <- 1
}

fn run(done chan int) {
    outer := fn() {
        if false {
        } else if m := map[string]int{"init": 0}; (fn() bool {
            task worker(m, done)
            return false
        })() {
        }
    }
    outer()
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, got {:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 when else-if init binding is captured by else-if condition closure, got {:?}",
        r.diagnostics
    );
}

#[test]
fn match_guard_closure_with_pattern_binding_rejects_e3001_445() {
    let r = check(
        r#"
fn worker(m map[string]int, done chan int) {
    m["worker"] = 1
    done <- 1
}

fn run(opt option[map[string]int], done chan int) {
    outer := fn() {
        match opt {
            some(m) if (fn() bool {
                task worker(m, done)
                return true
            })(): {
            }
            none: {}
        }
    }
    outer()
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, got {:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 when match arm pattern binding is captured by guard closure, got {:?}",
        r.diagnostics
    );
}

#[test]
fn else_if_cond_closure_grandchild_rejects_e3001_445() {
    let r = check(
        r#"
fn worker(m map[string]int, done chan int) {
    m["worker"] = 1
    done <- 1
}

fn run(done chan int) {
    m := map[string]int{"init": 0}
    outer := fn() {
        if false {
        } else if (fn() bool {
            inner := fn() {
                task worker(m, done)
            }
            inner()
            return false
        })() {
        }
    }
    outer()
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, got {:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 for grandchild closure inside else-if condition, got {:?}",
        r.diagnostics
    );
}

#[test]
fn else_if_cond_closure_spawning_primitive_accepts_445() {
    let r = check(
        r#"
fn worker(n int, done chan int) {
    done <- n
}

fn run(done chan int) {
    outer := fn() {
        if false {
        } else if (fn() bool {
            task worker(42, done)
            return false
        })() {
        }
    }
    outer()
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept, got {:?}", r.diagnostics);
}

// ===========================================================================
// Issue #449 — closure capture snapshotting, post-closure moves, and A3 scope
// ===========================================================================

#[test]
fn nested_closure_unaffected_by_later_local_shadow_rejects_e3001_449() {
    let r = check(
        r#"
fn worker(m map[string]int, done chan int) {
    m["worker"] = 1
    done <- 1
}

fn run(done chan int) {
    m := map[string]int{"k": 0}
    outer := fn() {
        inner := fn() {
            task worker(m, done)
        }
        m := 1
        _ = m
        inner()
    }
    outer()
    m["main"] = 2
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, got {:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 when later local shadow `m := 1` follows nested closure definition, got {:?}",
        r.diagnostics
    );
}

#[test]
fn nested_closure_sees_post_definition_move_in_subblock_and_peer_rejects_e3003_449() {
    let r = check(
        r#"
struct wrap {
    m map[string]int
}

fn consume(w wrap, done chan int) {
    w.m["worker"] = 1
    done <- 1
}

fn run(done chan int) {
    outer := fn() {
        if true {
            m := map[string]int{"k": 0}
            inner := fn() {
                _ = m["k"]
            }
            if true {
                w := wrap{m: m}
                task consume(move w, done)
            }
            inner()
        }
    }
    outer()
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, got {:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 when captured sub-block binding is moved via child-block may-share peer, got {:?}",
        r.diagnostics
    );
}

#[test]
fn a3_callee_composite_rebind_invalidates_slot_loan_e3102_449() {
    let r = check(
        r#"
struct cell {
    n int
}

struct holder {
    m map[int]cell
}

fn touch(m map[int]cell) {
    m = holder{m: m}.m
    m[2] = cell{n: 99}
}

fn main() {
    m := map[int]cell{1: cell{n: 10}}
    p := &m[1]
    touch(m)
    p.n = 20
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, got {:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 when callee rebinds param through composite literal projection and mutates it, got {:?}",
        r.diagnostics
    );
}

#[test]
fn a3_callee_branch_assign_to_named_result_invalidates_slot_loan_e3102_449() {
    let r = check(
        r#"
struct cell {
    n int
}

fn touch(m map[int]cell) (out map[int]cell) {
    if true {
        out = m
    }
    out[2] = cell{n: 99}
    return out
}

fn main() {
    m := map[int]cell{1: cell{n: 10}}
    p := &m[1]
    _ = touch(m)
    p.n = 20
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, got {:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 when callee assigns param to named result in a branch and mutates it, got {:?}",
        r.diagnostics
    );
}

#[test]
fn a3_closure_inner_block_shadow_does_not_mask_outer_mutation_e3102_449() {
    let r = check(
        r#"
struct cell {
    n int
}

struct holder {
    m map[int]cell
}

fn main() {
    m := map[int]cell{1: cell{n: 10}}
    p := &m[1]
    f := fn() {
        if true {
            m := 0
            _ = m
        }
        h := holder{m: m}
        h.m[2] = cell{n: 99}
    }
    f()
    p.n = 20
}
"#,
    );
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, got {:?}", r.diagnostics);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 when closure inner-block shadow exits before map mutation, got {:?}",
        r.diagnostics
    );
}
