//! Regression pin for #3273: channel send element-type soundness.
//!
//! `OpKind` collapses integer widths, fixed-size array dimensions and slice
//! element types, so a send whose value type differed from the channel element
//! type passed `hyb check` with 0 diagnostics and failed `go build`.

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn error_codes(src: &str) -> Vec<String> {
    let ast = parse_file("chan_send_3273.hy", src)
        .unwrap_or_else(|d| panic!("parse failed: {:?}", d));
    check_file(&ast)
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code)
        .collect()
}

/// A `Stmt::Send` with a mismatched integer width.
#[test]
fn send_statement_with_mismatched_scalar_width_is_rejected() {
    let got = error_codes(
        r#"fn main() {
    var ch chan int64 = make(chan int64, 1)
    var v int32 = 42
    ch <- v
}
"#,
    );
    assert!(
        got.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {got:?}"
    );
}

/// The same mismatch in a `select` send arm.
#[test]
fn select_send_with_mismatched_scalar_width_is_rejected() {
    let got = error_codes(
        r#"fn main() {
    var ch chan int64 = make(chan int64, 1)
    var v int32 = 42
    select {
        ch <- v: {}
        default: {}
    }
}
"#,
    );
    assert!(
        got.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {got:?}"
    );
}

/// Mismatched slice element width.
#[test]
fn send_of_slice_with_mismatched_element_width_is_rejected() {
    let got = error_codes(
        r#"fn main() {
    var ch chan []int64 = make(chan []int64, 1)
    var v []int32
    ch <- move v
}
"#,
    );
    assert!(
        got.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002, got {got:?}"
    );
}

/// Negative control: a matching width stays accepted.
#[test]
fn matching_scalar_width_stays_accepted() {
    let got = error_codes(
        r#"fn main() {
    var ch chan int64 = make(chan int64, 1)
    var v int64 = 42
    ch <- v
}
"#,
    );
    assert!(got.is_empty(), "expected no errors, got {got:?}");
}

/// Negative control: an untyped literal is assignable to any numeric element
/// type, so it must stay accepted.
#[test]
fn untyped_literal_send_stays_accepted() {
    let got = error_codes(
        r#"fn main() {
    ch := make(chan int, 1)
    ch <- 42
}
"#,
    );
    assert!(got.is_empty(), "expected no errors, got {got:?}");
}

/// Negative control: `byte` and `uint8` are the same Go type.
#[test]
fn byte_and_uint8_element_do_not_conflict() {
    let got = error_codes(
        r#"fn main() {
    bch := make(chan []byte, 1)
    var b []uint8
    bch <- move b
}
"#,
    );
    assert!(got.is_empty(), "expected no errors, got {got:?}");
}