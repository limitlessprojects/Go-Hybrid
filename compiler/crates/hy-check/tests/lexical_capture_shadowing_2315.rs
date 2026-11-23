//! Regression tests for issue #2315:
//! [soundness] hy-check: post-closure block shadowing and moves overwrite lexical capture environment (HYB-E3001 / HYB-E3003 bypass)

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn repro1_post_closure_shadow_bypassing_e3001_rejected() {
    // Repro 1 from #2315:
    // Outer `m` is map[string]int (non-sendable). Closure `f` captures `m` and spawns `task worker(m, done)`.
    // Later in the block, `m := 1` shadows `m` with sendable int.
    // Checker must reject with HYB-E3001 / concurrency boundary error.
    let src = r#"
fn worker(m map[string]int, done chan<- bool) {
    m["worker"] = 1
    done <- true
}
fn main() {
    m := map[string]int{"init": 0}
    done := make(chan bool, 1)
    if true {
        f := fn() {
            task worker(m, done)
        }
        f()
        m := 1
        _ = m
    }
    m["main"] = 2
    <-done
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected rejection of non-sendable map capture in task");
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E3001" || c == "HYB-E3002"),
        "expected HYB-E3001 or HYB-E3002 in diagnostics, got: {:?}",
        codes
    );
}

#[test]
fn repro2_post_closure_shadow_bypassing_e3003_rejected() {
    // Repro 2 from #2315:
    // Outer `m` is moved into task worker.
    // Closure `f` mutates moved map `m["closure"] = 2`.
    // Later in block, `m := map[string]int{"fresh": 3}` binds fresh map.
    // Checker must reject with HYB-E3003 (use/mutation of moved value).
    let src = r#"
fn worker(m map[string]int, done chan<- bool) {
    m["worker"] = 1
    done <- true
}
fn main() {
    m := map[string]int{"init": 0}
    done := make(chan bool, 1)
    task worker(move m, done)
    if true {
        f := fn() {
            m["closure"] = 2
        }
        f()
        m := map[string]int{"fresh": 3}
        _ = m
    }
    <-done
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected rejection of mutating moved map in closure");
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E3003" || c == "HYB-E3001"),
        "expected HYB-E3003 in diagnostics, got: {:?}",
        codes
    );
}

#[test]
fn post_closure_move_in_same_block_rejected() {
    // Variable captured by closure is subsequently moved in the same block.
    // The closure mutating the variable must be rejected.
    let src = r#"
fn worker(m map[string]int, done chan<- bool) {
    m["worker"] = 1
    done <- true
}
fn main() {
    m := map[string]int{"init": 0}
    done := make(chan bool, 1)
    if true {
        f := fn() {
            m["closure"] = 2
        }
        task worker(move m, done)
        f()
        m := 1
        _ = m
    }
    <-done
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E3003" || c == "HYB-E3001"),
        "expected move rejection, got: {:?}",
        codes
    );
}

#[test]
fn clean_closure_capture_without_mutation_accepted() {
    // Pure read or move with no shadowing works cleanly.
    let src = r#"
fn main() {
    x := 42
    if true {
        f := fn() int {
            return x
        }
        _ = f()
    }
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
}
