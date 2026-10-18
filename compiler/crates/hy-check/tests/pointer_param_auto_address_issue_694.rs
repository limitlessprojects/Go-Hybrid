use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src).unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

#[test]
fn test_issue_694_prog_hy_rejected_soundness() {
    let src = r#"
struct stats {
    n int
}

fn worker(s *stats, done chan<- bool) {
    s.n = 1
    done <- true
}

fn main() {
    const s = stats{n: 0}
    done := make(chan bool, 1)
    task worker(s, done)
    fmt.println(s.n)
    <-done
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "expected HYB-E3001 for mutable struct live borrow across tasks, got: {:?}",
        res.diagnostics
    );
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006 for const mutation via pointer parameter, got: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_issue_694_var_struct_task_rejected_e3001() {
    let src = r#"
struct stats {
    n int
}

fn worker(s *stats, done chan<- bool) {
    s.n = 1
    done <- true
}

fn main() {
    var s = stats{n: 0}
    done := make(chan bool, 1)
    task worker(s, done)
    fmt.println(s.n)
    <-done
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "expected HYB-E3001 for mutable struct live borrow across tasks, got: {:?}",
        res.diagnostics
    );
    assert!(
        !res.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "should not emit HYB-E3006 when s is a var, got: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_issue_694_const_struct_sync_call_mutating_rejected_e3006() {
    let src = r#"
struct stats {
    n int
}

fn worker(s *stats) {
    s.n = 1
}

fn main() {
    const s = stats{n: 0}
    worker(s)
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006 for synchronous const mutation via pointer parameter, got: {:?}",
        res.diagnostics
    );
    assert!(
        !res.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "should not emit HYB-E3001 on synchronous call, got: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_issue_694_const_struct_sync_call_read_only_accepted() {
    let src = r#"
struct stats {
    n int
}

fn reader(s *stats) {
    fmt.println(s.n)
}

fn main() {
    const s = stats{n: 0}
    reader(s)
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Accept);
    assert_eq!(res.diagnostics.len(), 0);
}

#[test]
fn test_issue_694_value_parameter_mutation_does_not_reject_const() {
    let src = r#"
struct stats {
    n int
}

fn worker(s stats) {
    s.n = 1
}

fn main() {
    const s = stats{n: 0}
    worker(s)
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Accept);
    assert_eq!(res.diagnostics.len(), 0);
}

#[test]
fn test_issue_694_task_value_copy_sendable_accepted() {
    let src = r#"
struct stats {
    n int
}

fn worker(s stats, done chan<- bool) {
    fmt.println(s.n)
    done <- true
}

fn main() {
    const s = stats{n: 0}
    done := make(chan bool, 1)
    task worker(s, done)
    <-done
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Accept);
    assert_eq!(res.diagnostics.len(), 0);
}

#[test]
fn test_issue_694_task_atomic_pointer_auto_address_accepted() {
    let src = r#"
fn worker(h *atomic.int64, done chan<- bool) {
    atomic.add_int64(h, 1)
    done <- true
}

fn main() {
    var hits atomic.int64
    done := make(chan bool, 1)
    task worker(hits, done)
    <-done
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Accept);
    assert_eq!(res.diagnostics.len(), 0);
}

#[test]
fn test_issue_694_managed_block_task_audited() {
    let src = r#"
struct stats {
    n int
}

fn worker(s *stats, done chan<- bool) {
    s.n = 1
    done <- true
}

fn main() {
    var s = stats{n: 0}
    done := make(chan bool, 1)
    managed("guarded shared stats", "mutex") {
        task worker(s, done)
    }
    <-done
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Accept);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E8001"),
        "expected HYB-E8001 managed warning, got: {:?}",
        res.diagnostics
    );
    assert!(
        !res.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "managed should suppress HYB-E3001 error, got: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_issue_694_method_auto_address_pointer_param() {
    let src = r#"
struct stats {
    n int
}

struct service {
    id int
}

fn (svc service) update(s *stats) {
    s.n = 1
}

fn main() {
    const s = stats{n: 0}
    svc := service{id: 1}
    svc.update(s)
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006 for method call mutating const via pointer param, got: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_issue_694_field_auto_address_pointer_param() {
    let src = r#"
struct inner {
    count int
}

struct outer {
    in inner
}

fn bump(c *inner) {
    c.count = 1
}

fn main() {
    const o = outer{in: inner{count: 0}}
    bump(o.in)
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006 for field auto-addressing mutating const, got: {:?}",
        res.diagnostics
    );
}

#[test]
fn test_issue_694_closure_auto_address_pointer_param() {
    let src = r#"
struct stats {
    n int
}

fn main() {
    const s = stats{n: 0}
    worker := fn(st *stats) {
        st.n = 1
    }
    worker(s)
}
"#;
    let res = check(src);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(
        res.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006 for closure call mutating const via pointer param, got: {:?}",
        res.diagnostics
    );
}
