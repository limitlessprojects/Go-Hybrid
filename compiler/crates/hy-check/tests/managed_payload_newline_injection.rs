//! Regression tests for `audit_managed_block` newline/CR and blank-ticket validation (`HYB-E0003`).
//!
//! `hy-emit` lowers `Stmt::Managed` metadata into a single-line Go comment
//! (`// managed [category] (reason) [tickets]`), and `hy-driver --report-managed`
//! prints one audit record per line. Any `\n` or `\r` embedded in `reason` or
//! `tickets` (or an empty/whitespace-only ticket string) must be rejected by
//! `hy-check` with `HYB-E0003` across both top-level functions and closures.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src)
        .unwrap_or_else(|diags| panic!("expected source to parse cleanly, got: {:?}", diags));
    check_file(&ast)
}

fn error_codes(res: &hy_check::CheckResult) -> Vec<String> {
    res.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn managed_reason_with_newline_or_carriage_return_rejects_e0003() {
    let lf = check(
        r#"
fn main() {
    managed("safe\nfmt.Println(\"INJECTED\")\n//", "mutex") {
    }
}
"#,
    );
    assert_eq!(lf.verdict, Verdict::Reject, "diags: {:?}", lf.diagnostics);
    assert!(
        error_codes(&lf).contains(&"HYB-E0003".to_string()),
        "expected HYB-E0003 for LF in managed reason, got: {:?}",
        lf.diagnostics
    );

    let cr = check(
        r#"
fn main() {
    managed("safe\rfake-line", "mutex") {
    }
}
"#,
    );
    assert_eq!(cr.verdict, Verdict::Reject, "diags: {:?}", cr.diagnostics);
    assert!(
        error_codes(&cr).contains(&"HYB-E0003".to_string()),
        "expected HYB-E0003 for CR in managed reason, got: {:?}",
        cr.diagnostics
    );
}

#[test]
fn managed_ticket_with_newline_or_blank_rejects_e0003() {
    let ticket_lf = check(
        r#"
fn main() {
    managed("mutex guards map", "mutex", "SEC-1\nfmt.Println(\"INJECTED\")") {
    }
}
"#,
    );
    assert_eq!(
        ticket_lf.verdict,
        Verdict::Reject,
        "diags: {:?}",
        ticket_lf.diagnostics
    );
    assert!(
        error_codes(&ticket_lf).contains(&"HYB-E0003".to_string()),
        "expected HYB-E0003 for LF in managed ticket, got: {:?}",
        ticket_lf.diagnostics
    );

    let ticket_blank = check(
        r#"
fn main() {
    managed("mutex guards map", "mutex", "   ") {
    }
}
"#,
    );
    assert_eq!(
        ticket_blank.verdict,
        Verdict::Reject,
        "diags: {:?}",
        ticket_blank.diagnostics
    );
    assert!(
        error_codes(&ticket_blank).contains(&"HYB-E0003".to_string()),
        "expected HYB-E0003 for blank managed ticket, got: {:?}",
        ticket_blank.diagnostics
    );
}

#[test]
fn closure_managed_block_with_newline_rejects_e0003_and_single_line_control_accepts() {
    let in_closure = check(
        r#"
fn main() {
    f := fn() {
        managed("ok\ninjected()", "mutex") {
        }
    }
    f()
}
"#,
    );
    assert_eq!(
        in_closure.verdict,
        Verdict::Reject,
        "diags: {:?}",
        in_closure.diagnostics
    );
    assert!(
        error_codes(&in_closure).contains(&"HYB-E0003".to_string()),
        "expected HYB-E0003 for newline in closure managed block, got: {:?}",
        in_closure.diagnostics
    );

    let ok = check(
        r#"
fn main() {
    managed("mutex guards shared counter", "mutex", "SEC-42") {
    }
}
"#,
    );
    assert_eq!(ok.verdict, Verdict::Accept, "diags: {:?}", ok.diagnostics);
    assert!(error_codes(&ok).is_empty(), "diags: {:?}", ok.diagnostics);
}
