use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    check_file(&parse_file("inline.hy", src).expect("must parse"))
}

#[test]
fn channel_element_primitive_mismatches_reject_e3011() {
    // Primitive element type mismatches (bool/int/float) must reject at send.
    let reject_cases = [
        r#"fn main() {
    ch := make(chan int, 1)
    ch <- true
}
"#,
        r#"fn main() {
    ch := make(chan bool, 1)
    ch <- 42
}
"#,
        r#"fn main() {
    ch := make(chan int, 1)
    var f float64 = 1.5
    ch <- f
}
"#,
    ];
    for src in reject_cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected REJECT for:
{}", src);
        assert!(r.diagnostics.iter().any(|d| d.severity == Severity::Error && d.code == "HYB-E3011"),
                "expected HYB-E3011 for:
{}
diags: {:?}", src, r.diagnostics);
    }

    let ok = check(r#"fn main() {
    ch_b := make(chan bool, 1)
    ch_b <- true
    b := <-ch_b
    ch_i := make(chan int, 1)
    ch_i <- 42
    var n int = <-ch_i
    fmt.println("{b} {n}")
}
"#);
    assert_eq!(ok.verdict, Verdict::Accept);
}
