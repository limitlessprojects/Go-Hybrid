use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn first_error_rule(r: &hy_check::CheckResult) -> Option<String> {
    r.diagnostics
        .iter()
        .find(|d| d.severity == Severity::Error)
        .and_then(|d| d.tier0_rule.clone())
}

#[test]
fn closure_body_loop_backedge_move_rejects_e3003_issue_196() {
    // Issue #196: moving a closure-local map inside a loop inside a closure
    // must reject across the loop backedge with HYB-E3003 (A1b).
    let r = check(
        "fn worker(m map[string]int, done chan<- bool) {\n\
           for i in 0..10 {\n\
             m[\"k\"] = i\n\
           }\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           done := make(chan bool, 10)\n\
           f := fn() {\n\
             agg := map[string]int{\"k\": 0}\n\
             for i in 0..2 {\n\
               task worker(move agg, done)\n\
             }\n\
           }\n\
           f()\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(first_error_rule(&r).as_deref(), Some("A1b"));
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003, got {:?}",
        r.diagnostics
    );
}

#[test]
fn closure_body_loop_fresh_local_per_iteration_accepts() {
    // Positive control: a fresh map defined inside the loop inside the closure
    // may be moved on every iteration.
    let r = check(
        "fn worker(m map[string]int, done chan<- bool) {\n\
           m[\"k\"] = 1\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           done := make(chan bool, 10)\n\
           f := fn() {\n\
             for i in 0..2 {\n\
               fresh := map[string]int{\"k\": i}\n\
               task worker(move fresh, done)\n\
             }\n\
           }\n\
           f()\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}
