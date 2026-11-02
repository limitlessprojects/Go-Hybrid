//! Regression tests for #3172: channel send statements and receive expressions
//! escape scalar width and array length checks (HYB-E1002, Row 13 BUILDABLE).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast =
        parse_file("inline.hy", src).unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_with(src: &str, code: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == code),
        "expected {}, got {:?}\nsrc:\n{}",
        code,
        cs,
        src
    );
    assert_eq!(r.verdict, Verdict::Reject, "src:\n{}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept, got {:?}\nsrc:\n{}",
        cs,
        src
    );
}

#[test]
fn t1_mismatched_scalar_in_send_statement() {
    rejects_with(
        r#"
fn main() {
    var ch chan int32
    var x int64 = 42
    ch <- x
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn t2_mismatched_scalar_in_select_send_arm() {
    rejects_with(
        r#"
fn main() {
    var ch chan int32
    var x int64 = 42
    select {
        ch <- x:
    }
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn t3_mismatched_array_length_in_send() {
    rejects_with(
        r#"
fn main() {
    var ch chan [2]int
    ch <- [3]int{1, 2, 3}
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn t4_mismatched_scalar_in_receive_assignment() {
    rejects_with(
        r#"
fn main() {
    var ch chan int32
    var y int64
    y = <-ch
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn t5_mismatched_scalar_in_receive_var_decl() {
    rejects_with(
        r#"
fn main() {
    var ch chan int32
    var y int64 = <-ch
    _ = y
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn t6_mismatched_array_length_in_receive_var_decl() {
    rejects_with(
        r#"
fn main() {
    var ch chan [2]int
    var y [3]int = <-ch
    _ = y
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn t7_control_valid_channel_operations_accepted() {
    accepts(
        r#"
fn main() {
    var ch chan int32
    var x int32 = 42
    ch <- move x
    ch <- 100
    var y int32 = <-ch
    _ = y

    var arr_ch chan [2]int
    var a [2]int = [2]int{1, 2}
    arr_ch <- move a
    var b [2]int = <-arr_ch
    _ = b

    var x2 int32 = 10
    select {
    ch <- move x2:
    val := <-ch:
        _ = val
    default:
    }
}
"#,
    );
}
