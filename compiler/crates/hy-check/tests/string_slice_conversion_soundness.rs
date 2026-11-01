use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn test_invalid_string_slice_conversions_rejected() {
    let src = r#"
type my_int64_slice = []int64
type my_float_slice = []float64
type my_string_slice = []string
type my_bool_slice = []bool

fn main() {
    var is []int64 = []int64{1, 2}
    var fs []float64 = []float64{1.0, 2.0}
    var ss []string = []string{"a", "b"}
    var bs []bool = []bool{true, false}

    _ = string(is)
    _ = string(fs)
    _ = string(ss)
    _ = string(bs)

    var s string = "hello"
    _ = my_int64_slice(s)
    _ = my_float_slice(s)
    _ = my_string_slice(s)
    _ = my_bool_slice(s)
}
"#;
    let ast = parse_file("t.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    assert_eq!(res.verdict, Verdict::Reject);
    assert_eq!(res.diagnostics.len(), 8);
    for d in &res.diagnostics {
        assert_eq!(d.code, "HYB-E1002");
    }
}

#[test]
fn test_valid_string_byte_rune_conversions_accepted() {
    let src = r#"
type my_byte_slice = []byte
type my_rune_slice = []rune

fn main() {
    var b []byte = []byte{104, 105}
    var r []rune = []rune{104, 105}

    _ = string(b)
    _ = string(r)

    var s string = "hello"
    _ = my_byte_slice(s)
    _ = my_rune_slice(s)
}
"#;
    let ast = parse_file("t.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    assert_eq!(res.verdict, Verdict::Accept, "valid byte/rune slice conversions must be accepted: {:?}", res.diagnostics);
    assert!(res.diagnostics.is_empty());
}

#[test]
fn test_array_string_conversion_rejected() {
    let src = r#"
fn main() {
    var arr [4]byte = [4]byte{1, 2, 3, 4}
    _ = string(arr)
}
"#;
    let ast = parse_file("t.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    assert_eq!(res.verdict, Verdict::Reject);
    assert!(res.diagnostics.iter().any(|d| d.code == "HYB-E1002"));
}
