//! inprocess_isolation.rs — verifies repeated and concurrent in-process checker invocations
//! for zero state leakage, zero drift, and deterministic verdicts (W-012 due diligence).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

const FILE_ACCEPT: &str = include_str!("../../../../tests/checker-corpus/16-move-send-clean.hy");
const FILE_REJECT: &str = include_str!("../../../../tests/checker-corpus/01-shared-map-across-tasks.hy");

#[test]
fn repeated_inprocess_checker_has_zero_state_drift() {
    let ast_accept = parse_file("accept.hy", FILE_ACCEPT).expect("parse accept");
    let ast_reject = parse_file("reject.hy", FILE_REJECT).expect("parse reject");

    // Run 1,000 alternating passes in process
    for i in 0..1000 {
        let res_accept = check_file(&ast_accept);
        assert_eq!(
            res_accept.verdict,
            Verdict::Accept,
            "iteration {}: accept verdict drifted!",
            i
        );
        let errs_accept: Vec<_> = res_accept
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert!(errs_accept.is_empty(), "iteration {}: leaked error into accept", i);

        let res_reject = check_file(&ast_reject);
        assert_eq!(
            res_reject.verdict,
            Verdict::Reject,
            "iteration {}: reject verdict drifted!",
            i
        );
        let errs_reject: Vec<_> = res_reject
            .diagnostics
            .iter()
            .filter(|d| d.severity == Severity::Error)
            .collect();
        assert_eq!(
            errs_reject.len(),
            1,
            "iteration {}: reject error count drifted!",
            i
        );
        assert_eq!(
            errs_reject[0].tier0_rule.as_deref(),
            Some("A1c"),
            "iteration {}: rule drifted!",
            i
        );
    }
}

#[test]
fn concurrent_inprocess_checking_is_thread_safe() {
    use std::thread;

    let handles: Vec<_> = (0..8)
        .map(|_| {
            thread::spawn(|| {
                let ast_accept = parse_file("accept.hy", FILE_ACCEPT).expect("parse accept");
                let ast_reject = parse_file("reject.hy", FILE_REJECT).expect("parse reject");
                for _ in 0..200 {
                    let r1 = check_file(&ast_accept);
                    assert_eq!(r1.verdict, Verdict::Accept);
                    let r2 = check_file(&ast_reject);
                    assert_eq!(r2.verdict, Verdict::Reject);
                }
            })
        })
        .collect();

    for h in handles {
        h.join().expect("thread join");
    }
}
