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
