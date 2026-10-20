//! p0_regressions_449.rs — regression tests for issue #449:
//! 1. Post-closure outer locals shadow nested captures (HYB-E3001/E3003)
//! 2. Block-scoped may-share/local in outer closure moved after nested closure (HYB-E3003)
//! 3. Callee rebinds param via composite/call (m = holder{m: m}.m) drops A3 slot-loan invalidation (HYB-E3102)
//! 4. Callee branch-assigns named result/outer variable (if true { out = m }; out[2] = 99) drops A3 slot-loan invalidation (HYB-E3102)
//! 5. Closure inner block shadows m (if true { m := 0 }) before mutating captured m[2] = 99 (HYB-E3102)

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
fn repro1_late_shadow_in_outer_closure_rejects_e3001() {
    let src = "\
fn worker(m map[string]int, done chan<- bool) {
    for i in 0..2000 {
        m[\"k\"] = i
    }
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
    for i in 0..2000 {
        m[\"k\"] = i
    }
    <-done
}
";
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 for unmoved outer capture in nested closure, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn repro1_outer_closure_move_after_nested_closure_rejects_e3003() {
    let src = "\
fn worker(m map[string]int, done chan<- bool) {
    m[\"k\"] = 1
    done <- true
}

fn main() {
    done := make(chan bool, 1)
    outer := fn() {
        m := map[string]int{\"k\": 0}
        inner := fn() {
            m[\"k\"] = 2
        }
        task worker(move m, done)
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
        "expected HYB-E3003 for nested closure mutating outer local after move, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn repro1_outer_closure_move_after_child_block_peer_closure_rejects_e3003() {
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
        "expected HYB-E3003 for mutating may-share peer of moved map in nested closure, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn repro2_callee_composite_rebind_rejects_e3102() {
    let src = "\
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
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 for slot loan invalidated by callee composite rebind, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn repro2_callee_branch_assign_outer_var_rejects_e3102() {
    let src = "\
fn touch(m map[int]int) {
    var out map[int]int
    if true {
        out = m
    }
    out[2] = 99
}

fn main() {
    m := map[int]int{1: 10}
    p := &m[1]
    touch(m)
    fmt.println(p)
}
";
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 for slot loan invalidated through branch-assigned outer alias, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn repro3_closure_block_shadow_rejects_e3102() {
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
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 for slot loan invalidated by closure mutating outer m after block shadow, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn repro3_closure_composite_literal_alias_rejects_e3102() {
    let src = "\
struct holder { m map[int]int }

fn main() {
    m := map[int]int{1: 10}
    p := &m[1]
    bump := fn() {
        out := holder{m: m}
        out.m[2] = 99
    }
    bump()
    fmt.println(p)
}
";
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 for slot loan invalidated by closure mutating m through composite alias, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}
