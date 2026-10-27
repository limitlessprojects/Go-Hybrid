//! slot_loan_move_reject.rs — regression tests for issue #2066:
//! moving an active O8 map-slot loan (`task w(move s)` / `ch <- move s`)
//! bypasses HYB-E3104 and races with store-back (data race).
//!
//! A map-slot loan lowers to a function-local copy of the slot plus an
//! automatic store-back `m[k] = s` at its last use. Transferring ownership
//! via `move` at a concurrency boundary (`task`, `send`, `select-send`) or
//! local move re-binding escapes the loan or races with the store-back.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn error_messages(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.message.clone())
        .collect()
}

#[test]
fn task_spawn_moving_slot_loan_is_rejected_e3104() {
    let src = r#"
struct record { count int }
fn worker(s record, done chan bool) { done <- true }

fn test() {
    m := map[string]record{"a": record{count: 1}}
    s := &m["a"]
    done := make(chan bool, 1)
    task worker(move s, done)
    <-done
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E3104".to_string()));
    let msgs = error_messages(&r);
    assert!(
        msgs.iter().any(|m| m.contains("moves an active map-slot loan")),
        "diagnostic message must describe move of loan, got: {:?}",
        msgs
    );
}

#[test]
fn chan_send_moving_slot_loan_is_rejected_e3104() {
    let src = r#"
struct record { count int }

fn test() {
    m := map[string]record{"a": record{count: 1}}
    s := &m["a"]
    ch := make(chan record, 1)
    ch <- move s
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E3104".to_string()));
    let msgs = error_messages(&r);
    assert!(
        msgs.iter().any(|m| m.contains("moves an active map-slot loan")),
        "diagnostic message must describe move of loan, got: {:?}",
        msgs
    );
}

#[test]
fn chan_send_parenthesized_move_is_rejected_e3104() {
    let src = r#"
struct record { count int }

fn test() {
    m := map[string]record{"a": record{count: 1}}
    s := &m["a"]
    ch := make(chan record, 1)
    ch <- move (s)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E3104".to_string()));
}

#[test]
fn task_spawn_parenthesized_move_is_rejected_e3104() {
    let src = r#"
struct record { count int }
fn worker(s record, done chan bool) { done <- true }

fn test() {
    m := map[string]record{"a": record{count: 1}}
    s := &m["a"]
    done := make(chan bool, 1)
    task worker(move (s), done)
    <-done
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E3104".to_string()));
}

#[test]
fn select_arm_send_moving_slot_loan_is_rejected_e3104() {
    let src = r#"
struct record { count int }

fn test() {
    m := map[string]record{"a": record{count: 1}}
    s := &m["a"]
    ch := make(chan record, 1)
    select {
    ch <- move s:
        fmt.println("sent")
    default:
        fmt.println("default")
    }
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E3104".to_string()));
}

#[test]
fn local_define_moving_slot_loan_is_rejected_e3104() {
    let src = r#"
struct record { count int }

fn test() {
    m := map[string]record{"a": record{count: 1}}
    s := &m["a"]
    y := move s
    fmt.println(y)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E3104".to_string()));
}

#[test]
fn normal_slot_loan_mutation_and_use_stays_accepted() {
    let src = r#"
struct record { count int }

fn test() {
    m := map[string]record{"a": record{count: 1}}
    s := &m["a"]
    s.count = 42
    fmt.println(s.count)
}
"#;
    let r = check(src);
    assert!(
        error_codes(&r).is_empty(),
        "normal slot loan mutation should be accepted, got: {:?}",
        r.diagnostics
    );
}
