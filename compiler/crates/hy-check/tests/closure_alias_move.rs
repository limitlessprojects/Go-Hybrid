//! closure_alias_move.rs — regression tests for issue #137:
//! local rebinds inside closures bypass alias tracking and non_owned_origin,
//! allowing unsound task spawns.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
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

const CLOSURE_REBIND_ALIAS_REPRO: &str = "\
fn worker(m map[string]int, done chan<- bool) {
    for i in 0..1000 {
        m[\"k\"] = i
    }
    done <- true
}

fn main() {
    m := map[string]int{\"k\": 0}
    done := make(chan bool, 2)

    spawn_it := fn() {
        alias := m
        task worker(move alias, done)
    }

    spawn_it()
    spawn_it()
    <-done
    <-done
}
";

#[test]
fn closure_rebind_alias_move_is_rejected_e3004() {
    let r = check(CLOSURE_REBIND_ALIAS_REPRO);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3004"),
        "expected HYB-E3004 for moving a local alias of a captured binding, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
    let e3004 = r
        .diagnostics
        .iter()
        .find(|d| d.code == "HYB-E3004")
        .expect("HYB-E3004 must be present");
    assert_eq!(
        e3004.tier0_rule.as_deref(),
        Some("A1b"),
        "non-owned move rejects under Tier-0 rule A1b"
    );
}

#[test]
fn closure_local_owned_move_is_accepted() {
    // Pure closure-local binding (not an alias of captured state) can still be moved
    let src = "\
fn worker(m map[string]int, done chan<- bool) {
    done <- true
}

fn main() {
    done := make(chan bool, 1)
    spawn_it := fn() {
        fresh := map[string]int{\"k\": 0}
        task worker(move fresh, done)
    }
    spawn_it()
    <-done
}
";
    let r = check(src);
    let cs = codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E3004"),
        "pure closure-local binding should not be rejected, got {:?}",
        cs
    );
}

const CLOSURE_REASSIGN_ALIAS_REPRO: &str = "\
fn worker(m map[string]int, done chan bool) {
    for i in 0..1000 {
        m[\"k\"] = i
    }
    done <- true
}

fn main() {
    m := map[string]int{\"k\": 0}
    done := make(chan bool, 2)

    spawn_it := fn() {
        alias := map[string]int{\"k\": 1}
        alias = m
        task worker(move alias, done)
    }

    spawn_it()
    spawn_it()
    <-done
    <-done
}
";

#[test]
fn closure_reassign_captured_alias_move_is_rejected_e3004() {
    let r = check(CLOSURE_REASSIGN_ALIAS_REPRO);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3004"),
        "expected HYB-E3004 for moving a closure-local binding reassigned to captured binding, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
    let e3004 = r
        .diagnostics
        .iter()
        .find(|d| d.code == "HYB-E3004")
        .expect("HYB-E3004 must be present");
    assert_eq!(
        e3004.tier0_rule.as_deref(),
        Some("A1b"),
        "non-owned move rejects under Tier-0 rule A1b"
    );
}
