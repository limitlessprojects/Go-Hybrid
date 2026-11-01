use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn test_invalid_mismatched_slice_conversions_rejected() {
    let src = r#"
type int_slice_a = []int32
type int_slice_b = []int64
type float_slice_a = []float32
type float_slice_b = []float64

fn main() {
    var a int_slice_a
    var b int_slice_b = int_slice_b(a)

    var f int_slice_b
    var g int_slice_a = int_slice_a(f)

    var fa float_slice_a
    var fb float_slice_b = float_slice_b(fa)

    _ = b
    _ = g
    _ = fb
}
"#;
    let ast = parse_file("invalid_slice_conv.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    println!("Verdict: {:?}", res.verdict);
    for d in &res.diagnostics {
        println!("Diag: {} - {}", d.code, d.message);
    }
    assert_eq!(res.verdict, Verdict::Reject);
    assert_eq!(res.diagnostics.len(), 3);
    for d in &res.diagnostics {
        assert_eq!(d.code, "HYB-E1002");
    }
}

#[test]
fn test_valid_same_element_slice_conversions_accepted() {
    let src = r#"
type int_slice_custom = []int64

fn main() {
    var xs []int64 = []int64{ 1, 2, 3 }
    var ys int_slice_custom = int_slice_custom(xs)
    _ = ys
}
"#;
    let ast = parse_file("valid_slice_conv.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    for d in &res.diagnostics {
        println!("Unexpected Diag: {} - {}", d.code, d.message);
    }
    assert_eq!(res.verdict, Verdict::Accept);
    assert!(res.diagnostics.is_empty());
}
