//! DRAFT (FOM2/Fo, 2026-11-10 05:05 IST — NOT SUBMITTED; needs QC green on a
//! C-toolchain env or fork CI before any PR). Modeled on
//! tests/foreign_io_method_surfaces.rs: guarantees that calling phantom /
//! unsupported methods on the curated `time.ticker` / `time.timer` foreign
//! surfaces rejects fail-closed with HYB-E1002 (Row 13 BUILDABLE).
//!
//! Surfaces under test (declared in hy-check/src/foreign.rs, TIME_TICKER /
//! TIME_TIMER, foreign.rs:418): both ["stop", "reset"]. No dedicated
//! surface-test file existed before this draft.

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn phantom_ticker_methods_are_rejected() {
    let bad = check(
        "import \"time\"\n\
         fn inspect(t *time.ticker) {\n\
             t.totally_fake()\n\
         }\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject);
    assert!(codes(&bad).iter().any(|c| c == "HYB-E1002"));
    let diag = bad.diagnostics.iter().find(|d| d.code == "HYB-E1002").unwrap();
    assert!(diag.message.contains("time.ticker"), "{}", diag.message);
}

#[test]
fn phantom_timer_methods_are_rejected() {
    let bad = check(
        "import \"time\"\n\
         fn inspect(t *time.timer) {\n\
             t.bogus_timer_op()\n\
         }\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject);
    assert!(codes(&bad).iter().any(|c| c == "HYB-E1002"));
    let diag = bad.diagnostics.iter().find(|d| d.code == "HYB-E1002").unwrap();
    assert!(diag.message.contains("time.timer"), "{}", diag.message);
}
