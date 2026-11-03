use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> (Verdict, Vec<hy_diag::Diagnostic>) {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    (res.verdict, res.diagnostics)
}

#[test]
fn send_stmt_type_mismatch_rejected() {
    let cases = [
        // float variable sent to int chan
        r#"
fn main() {
    ch := make(chan int, 1)
    var f float64 = 1.5
    ch <- f
}
"#,
        // non-const int variable sent to float chan
        r#"
fn main() {
    ch := make(chan float64, 1)
    var i int = 1
    ch <- i
}
"#,
        // float literal sent to int chan
        r#"
fn main() {
    ch := make(chan int, 1)
    ch <- 1.5
}
"#,
        // int literal sent to string chan
        r#"
fn main() {
    ch := make(chan string, 1)
    ch <- 123
}
"#,
    ];

    for src in cases {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Reject,
            "expected reject for mismatched send operand:\n{src}"
        );
        assert!(
            diags.iter().any(|d| d.code == "HYB-E1002"),
            "expected HYB-E1002 in diags: {:?}",
            diags
        );
    }
}

#[test]
fn select_arm_send_type_mismatch_rejected() {
    let cases = [
        // float variable sent to int chan in select
        r#"
fn main() {
    ch := make(chan int, 1)
    var f float64 = 1.5
    select {
        ch <- f:
    }
}
"#,
        // non-const int variable sent to float chan in select
        r#"
fn main() {
    ch := make(chan float64, 1)
    var i int = 1
    select {
        ch <- i:
    }
}
"#,
        // float literal sent to int chan in select
        r#"
fn main() {
    ch := make(chan int, 1)
    select {
        ch <- 1.5:
    }
}
"#,
    ];

    for src in cases {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Reject,
            "expected reject for mismatched select send arm:\n{src}"
        );
        assert!(
            diags.iter().any(|d| d.code == "HYB-E1002"),
            "expected HYB-E1002 in diags: {:?}",
            diags
        );
    }
}

#[test]
fn send_compatible_and_untyped_const_accepted() {
    let src = r#"
const c = 2

fn main() {
    ch_i := make(chan int, 1)
    ch_f := make(chan float64, 1)
    ch_s := make(chan string, 1)

    var i int = 1
    var f float64 = 2.0
    var s string = "hello"

    // Same types
    ch_i <- i
    ch_f <- f
    ch_s <- s

    // Untyped integer literal/constant sent to float chan
    ch_f <- 1
    ch_f <- c

    // In select
    select {
        ch_i <- i:
        ch_f <- 1:
        ch_s <- s:
    }
}
"#;
    let (verdict, diags) = check(src);
    assert_eq!(
        verdict,
        Verdict::Accept,
        "expected accept for compatible channel sends: {:?}",
        diags
    );
}
