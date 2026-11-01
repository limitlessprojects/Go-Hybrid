use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn test_invalid_copy_element_types_rejected() {
    let src = r#"
type my_int64_slice = []int64
type my_int32_slice = []int32

fn main() {
    var dst []int64 = []int64{1, 2}
    var src []int32 = []int32{3, 4}
    _ = copy(dst, src)

    var dst_f64 []float64 = []float64{1.0, 2.0}
    var src_f32 []float32 = []float32{3.0, 4.0}
    _ = copy(dst_f64, src_f32)

    var dst_int []int = []int{1, 2}
    var str string = "hi"
    _ = copy(dst_int, str)

    var dst_i64 []int64 = []int64{1, 2}
    _ = copy(dst_i64, str)

    var dst_rune []rune = []rune{1, 2}
    _ = copy(dst_rune, str)

    var a_dst my_int64_slice = []int64{1, 2}
    var a_src my_int32_slice = []int32{3, 4}
    _ = copy(a_dst, a_src)
}
"#;
    let ast = parse_file("t.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    println!("Verdict: {:?}", res.verdict);
    for d in &res.diagnostics {
        println!("Diag: {} - {}", d.code, d.message);
    }
    assert_eq!(res.verdict, Verdict::Reject);
    assert_eq!(res.diagnostics.len(), 6);
    for d in &res.diagnostics {
        assert_eq!(d.code, "HYB-E1002");
    }
}

#[test]
fn test_valid_copy_accepted() {
    let src = r#"
type byte_slice = []byte
type uint8_slice = []uint8
type rune_slice = []rune
type int32_slice = []int32

fn main() {
    var d64 []int64 = []int64{1, 2}
    var s64 []int64 = []int64{3, 4}
    _ = copy(d64, s64)

    var df []float64 = []float64{1.0, 2.0}
    var sf []float64 = []float64{3.0, 4.0}
    _ = copy(df, sf)

    var b []byte = []byte{1, 2}
    var str string = "hi"
    _ = copy(b, str)

    var ub uint8_slice = []uint8{1, 2}
    var bb byte_slice = []byte{3, 4}
    _ = copy(ub, bb)

    var r rune_slice = []rune{1, 2}
    var i32 int32_slice = []int32{3, 4}
    _ = copy(r, i32)
}
"#;
    let ast = parse_file("t.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "valid copy calls must be accepted: {:?}",
        res.diagnostics
    );
    assert!(res.diagnostics.is_empty());
}
