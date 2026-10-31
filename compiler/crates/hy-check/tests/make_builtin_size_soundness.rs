//! Regression tests for #2540:
//! `make` size and capacity arguments must be of integer type,
//! preventing non-integer variables from escaping to go build.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> (Verdict, Vec<hy_diag::Diagnostic>) {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    (res.verdict, res.diagnostics)
}

#[test]
fn make_non_integer_variable_size_rejected_e2005() {
    let cases = [
        // 1. Slice size as float64 variable
        r#"
fn main() {
    var f float64 = 3.5
    _ = make([]int, f)
}
"#,
        // 2. Slice capacity as float64 variable
        r#"
fn main() {
    var f float64 = 10.0
    _ = make([]int, 1, f)
}
"#,
        // 3. Slice size as string variable
        r#"
fn main() {
    var s string = "10"
    _ = make([]int, s)
}
"#,
        // 4. Slice size as bool variable
        r#"
fn main() {
    var b bool = true
    _ = make([]int, b)
}
"#,
        // 5. Map size as float64 variable
        r#"
fn main() {
    var f float64 = 5.0
    _ = make(map[string]int{}, f)
}
"#,
        // 6. Channel buffer size as float64 variable
        r#"
fn main() {
    var f float64 = 2.0
    _ = make(chan int, f)
}
"#,
    ];

    for (idx, src) in cases.iter().enumerate() {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Reject,
            "case #{idx} should reject, got Accept:\n{src}"
        );
        let has_e2005 = diags.iter().any(|d| d.code == "HYB-E2005");
        assert!(
            has_e2005,
            "case #{idx} should emit HYB-E2005, got: {diags:?}\n{src}"
        );
    }
}

#[test]
fn make_integer_size_accepted() {
    let cases = [
        // 1. Slice with int variable length
        r#"
fn main() {
    var n int = 5
    _ = make([]int, n)
}
"#,
        // 2. Slice with int variable len and cap
        r#"
fn main() {
    var n int = 5
    var c int = 10
    _ = make([]int, n, c)
}
"#,
        // 3. Slice with int constant
        r#"
const n = 5
fn main() {
    _ = make([]int, n)
}
"#,
        // 4. Map with int variable
        r#"
fn main() {
    var n int = 10
    _ = make(map[string]int{}, n)
}
"#,
        // 5. Channel with int variable
        r#"
fn main() {
    var n int = 2
    _ = make(chan int, n)
}
"#,
    ];

    for (idx, src) in cases.iter().enumerate() {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Accept,
            "case #{idx} should accept, got Reject with diags: {diags:?}\n{src}"
        );
    }
}
