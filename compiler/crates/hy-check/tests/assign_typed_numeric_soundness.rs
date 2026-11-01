use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn test_invalid_mismatched_numeric_assignments_rejected() {
    let src = r#"
fn main() {
    var a64 int64 = 10
    var b32 int32 = 3
    var f64 float64 = 1.0
    var f32 float32 = 2.0

    // Single assignments across distinct numeric widths
    a64 = b32
    f64 = f32

    // Parallel assignments across distinct numeric widths
    a64, f64 = b32, f32

    _ = a64
    _ = f64
}
"#;
    let ast = parse_file("t.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    println!("Verdict: {:?}", res.verdict);
    for d in &res.diagnostics {
        println!("Diag: {} - {}", d.code, d.message);
    }
    assert_eq!(res.verdict, Verdict::Reject);
    assert_eq!(res.diagnostics.len(), 4);
    for d in &res.diagnostics {
        assert_eq!(d.code, "HYB-E1002");
    }
}

#[test]
fn test_valid_numeric_assignments_accepted() {
    let src = r#"
fn main() {
    var a64 int64 = 10
    var b64 int64 = 20
    var f64 float64 = 1.0
    var g64 float64 = 2.0

    // Same-type assignments
    a64 = b64
    f64 = g64
    a64, f64 = b64, g64

    // Untyped constants adapt legally
    a64 = 100
    f64 = 3.14
    a64, f64 = 200, 6.28

    // Official type aliases
    var b byte = 1
    var u uint8 = 2
    b = u
    u = b

    var r rune = 10
    var i int32 = 20
    r = i
    i = r

    _ = a64
    _ = f64
    _ = b
    _ = u
    _ = r
    _ = i
}
"#;
    let ast = parse_file("t.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    for d in &res.diagnostics {
        println!("Unexpected Diag: {} - {}", d.code, d.message);
    }
    assert_eq!(res.verdict, Verdict::Accept);
    assert!(res.diagnostics.is_empty());
}
