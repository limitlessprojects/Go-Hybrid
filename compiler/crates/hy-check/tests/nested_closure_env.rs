//! nested_closure_env.rs — regression tests for nested closure capture
//! environments (outer-closure params, locals, aliases, and moves must
//! propagate to nested closures rather than resolving against the top-level
//! function's FnState).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn nested_closure_sees_outer_closure_param_shadowing_primitive_e3001() {
    let src = "\
fn worker(m map[string]int, done chan<- bool) {
    m[\"k\"] = 1
    done <- true
}

fn main() {
    done := make(chan bool, 1)
    m := 1
    fmt.println(m)

    outer := fn(m map[string]int) {
        inner := fn() {
            task worker(m, done)
        }
        inner()
    }

    shared := map[string]int{\"k\": 0}
    outer(shared)
    <-done
}
";
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 for unmoved outer-closure map param in nested closure, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn nested_closure_alias_move_of_outer_closure_local_is_rejected_e3004() {
    let src = "\
fn worker(m map[string]int, done chan<- bool) {
    m[\"k\"] = 1
    done <- true
}

fn main() {
    done := make(chan bool, 2)
    m := 1
    fmt.println(m)

    outer := fn() {
        m := map[string]int{\"k\": 0}
        inner := fn() {
            alias := m
            task worker(move alias, done)
        }
        inner()
        inner()
    }

    outer()
    <-done
    <-done
}
";
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3004"),
        "expected HYB-E3004 for moving alias of outer-closure local inside nested closure, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn nested_closure_use_after_outer_closure_move_is_rejected_e3003() {
    let src = "\
fn worker(m map[string]int, done chan<- bool) {
    m[\"k\"] = 1
    done <- true
}

fn main() {
    done := make(chan bool, 1)
    m := 1
    fmt.println(m)

    outer := fn() {
        m := map[string]int{\"k\": 0}
        task worker(move m, done)
        inner := fn() {
            m[\"k\"] = 2
        }
        inner()
    }

    outer()
    <-done
}
";
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 for nested closure mutating outer-closure local after move, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn late_local_in_outer_closure_does_not_shadow_nested_closure_capture_e3001() {
    let src = "\
fn worker(m map[string]int, done chan<- bool) {
    m[\"k\"] = 1
    done <- true
}

fn main() {
    done := make(chan bool, 1)
    m := map[string]int{\"k\": 0}
    outer := fn() {
        inner := fn() {
            task worker(m, done)
        }
        m := 1
        fmt.println(m)
        inner()
    }
    outer()
    m[\"k\"] = 2
    <-done
}
";
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 when m := 1 is declared after nested closure definition, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn nested_closure_in_child_block_sees_post_block_move_of_may_share_peer_e3003() {
    let src = "\
struct holder { m map[string]int }
fn worker(m map[string]int, done chan<- bool) {
    m[\"k\"] = 1
    done <- true
}

fn main() {
    done := make(chan bool, 1)
    outer := fn() {
        m := map[string]int{\"k\": 0}
        var run fn()
        if true {
            h := holder{m: m}
            run = fn() {
                h.m[\"k\"] = 2
            }
        }
        task worker(move m, done)
        run()
    }
    outer()
    <-done
}
";
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 for nested closure mutating block-local may-share peer after outer move, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn nested_closure_in_subblock_sees_post_move_of_block_local_map_e3003() {
    let src = "\
fn worker(m map[string]int, done chan<- bool) {
    m[\"k\"] = 1
    done <- true
}

fn main() {
    done := make(chan bool, 1)
    outer := fn() {
        var run fn()
        if true {
            m := map[string]int{\"k\": 0}
            if true {
                run = fn() {
                    m[\"k\"] = 2
                }
            }
            task worker(move m, done)
        }
        run()
    }
    outer()
    <-done
}
";
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 for nested closure mutating outer block-local map moved after inner block, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn a3_callee_rebind_via_composite_or_call_still_invalidates_slot_loan_e3102() {
    let src_struct = "\
struct holder { m map[int]int }
fn touch(m map[int]int) {
    m = holder{m: m}.m
    m[2] = 99
}
fn main() {
    m := map[int]int{1: 10}
    p := &m[1]
    touch(m)
    fmt.println(p)
}
";
    let r1 = check(src_struct);
    assert!(
        codes(&r1).iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 on struct-field self-rebind in callee, got {:?}",
        codes(&r1)
    );

    let src_branch_assign = "\
fn touch(m map[int]int) (out map[int]int) {
    if true {
        out = m
    }
    out[2] = 99
    return
}
fn main() {
    m := map[int]int{1: 10}
    p := &m[1]
    _ = touch(m)
    fmt.println(p)
}
";
    let r2 = check(src_branch_assign);
    assert!(
        codes(&r2).iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 on branch-assigned named result mutated after branch, got {:?}",
        codes(&r2)
    );
}

#[test]
fn a3_closure_inner_block_shadow_does_not_mask_subsequent_capture_mutation_e3102() {
    let src = "\
fn main() {
    m := map[int]int{1: 10}
    p := &m[1]
    bump := fn() {
        if true {
            m := 0
            fmt.println(m)
        }
        m[2] = 99
    }
    bump()
    fmt.println(p)
}
";
    let r = check(src);
    assert!(
        codes(&r).iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 when inner block shadows `m` and outer closure body mutates captured `m`, got {:?}",
        codes(&r)
    );
}

