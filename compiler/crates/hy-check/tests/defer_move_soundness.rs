//! defer_move_soundness.rs — regression tests for issue #415:
//! deferred call keeps a live reference, so moving the value
//! into a task (or sending on a channel / moving locally) races
//! and reads a moved value.

use hy_check::{check_file, CheckResult};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn defer_arg_spawn_move_rejects_e3003_issue_415_repro() {
    let src = "\
fn report(m map[string]int) int {
    return m[\"task\"]
}

fn worker(m map[string]int) {
    m[\"task\"]++
}

fn main() {
    shared := map[string]int{}
    defer report(shared)

    task worker(move shared)
}
";
    let r = check(src);
    let c = error_codes(&r);
    assert!(
        c.contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003 for deferred read of moved map, got: {:?}",
        c
    );
    let diag = r
        .diagnostics
        .iter()
        .find(|d| d.code == "HYB-E3003")
        .expect("HYB-E3003 diagnostic must be present");
    assert_eq!(diag.tier0_rule.as_deref(), Some("A1b"));
}

#[test]
fn defer_arg_send_move_rejects_e3003() {
    let src = "\
fn report(m map[string]int) int {
    return m[\"task\"]
}

fn main() {
    ch := make(chan map[string]int, 1)
    shared := map[string]int{}
    defer report(shared)

    ch <- move shared
}
";
    let r = check(src);
    let c = error_codes(&r);
    assert!(
        c.contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003 for deferred read of channel-moved map, got: {:?}",
        c
    );
}

#[test]
fn defer_arg_may_share_alias_move_rejects_e3003() {
    let src = "\
fn report(m map[string]int) int {
    return m[\"task\"]
}

fn worker(m map[string]int) {
    m[\"task\"]++
}

fn main() {
    shared := map[string]int{}
    m2 := shared
    defer report(m2)

    task worker(move shared)
}
";
    let r = check(src);
    let c = error_codes(&r);
    assert!(
        c.contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003 for deferred read of may-share alias of moved map, got: {:?}",
        c
    );
}

#[test]
fn defer_arg_reverse_may_share_alias_move_rejects_e3003() {
    let src = "\
fn report(m map[string]int) int {
    return m[\"task\"]
}

fn worker(m map[string]int) {
    m[\"task\"]++
}

fn main() {
    shared := map[string]int{}
    m2 := shared
    defer report(shared)

    task worker(move m2)
}
";
    let r = check(src);
    let c = error_codes(&r);
    assert!(
        c.contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003 for deferred read of origin when alias moved, got: {:?}",
        c
    );
}

#[test]
fn defer_arg_local_move_rejects_e3003() {
    let src = "\
fn report(m map[string]int) int {
    return m[\"task\"]
}

fn main() {
    shared := map[string]int{}
    defer report(shared)

    m2 := move shared
}
";
    let r = check(src);
    let c = error_codes(&r);
    assert!(
        c.contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003 for deferred read after local move, got: {:?}",
        c
    );
}

#[test]
fn defer_arg_different_binding_stays_accepted() {
    let src = "\
fn report(m map[string]int) int {
    return m[\"task\"]
}

fn worker(m map[string]int) {
    m[\"task\"]++
}

fn main() {
    shared := map[string]int{}
    other := map[string]int{}
    defer report(other)

    task worker(move shared)
}
";
    let r = check(src);
    let c = error_codes(&r);
    assert!(
        c.is_empty(),
        "expected 0 diagnostics when defer reads different map, got: {:?}",
        c
    );
}

#[test]
fn defer_arg_primitive_value_stays_accepted() {
    let src = "\
fn report(n int) int {
    return n
}

fn worker(m map[string]int) {
    m[\"task\"]++
}

fn main() {
    shared := map[string]int{}
    n := 42
    defer report(n)

    task worker(move shared)
}
";
    let r = check(src);
    let c = error_codes(&r);
    assert!(
        c.is_empty(),
        "expected 0 diagnostics when defer reads primitive int, got: {:?}",
        c
    );
}

#[test]
fn defer_closure_twin_still_rejects_e3003() {
    let src = "\
fn worker(m map[string]int) {
    m[\"task\"]++
}

fn main() {
    shared := map[string]int{}
    show := fn() { _ = shared[\"task\"] }
    defer show()

    task worker(move shared)
}
";
    let r = check(src);
    let c = error_codes(&r);
    assert!(
        c.contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003 for closure capture defer twin, got: {:?}",
        c
    );
}

#[test]
fn defer_inside_closure_body_rejects_e3003() {
    let src = "\
fn report(m map[string]int) int {
    return m[\"task\"]
}

fn worker(m map[string]int) {
    m[\"task\"]++
}

fn main() {
    f := fn() {
        shared := map[string]int{}
        defer report(shared)
        task worker(move shared)
    }
    f()
}
";
    let r = check(src);
    let c = error_codes(&r);
    assert!(
        c.contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003 for defer inside closure body, got: {:?}",
        c
    );
}
