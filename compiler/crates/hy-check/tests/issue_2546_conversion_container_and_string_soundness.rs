//! Regression tests for #2546 & #2531:
//! Type conversion between string and slice/array must validate element kinds,
//! and container conversions (slice, array, map, chan) must reject mismatched elements with HYB-E1002.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> (Verdict, Vec<hy_diag::Diagnostic>) {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    (res.verdict, res.diagnostics)
}

#[test]
fn string_conversion_from_invalid_slice_or_array_rejected_with_e1002() {
    let cases = [
        // 1. cannot convert []float64 to string
        r#"
fn main() {
    fs := []float64{1.0, 2.0}
    _ = string(fs)
}
"#,
        // 2. cannot convert []string to string
        r#"
fn main() {
    ss := []string{"a", "b"}
    _ = string(ss)
}
"#,
        // 3. cannot convert []bool to string
        r#"
fn main() {
    bs := []bool{true, false}
    _ = string(bs)
}
"#,
        // 4. cannot convert [N]int array directly to string
        r#"
fn main() {
    arr := [2]int{65, 66}
    _ = string(arr)
}
"#,
    ];

    for (idx, src) in cases.iter().enumerate() {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Reject,
            "invalid string conversion #{idx} should reject, got Accept:\n{src}"
        );
        assert!(
            diags.iter().any(|d| d.code == "HYB-E1002"),
            "invalid string conversion #{idx} should emit HYB-E1002: {diags:?}\n{src}"
        );
    }
}

#[test]
fn slice_array_conversion_from_string_invalid_element_rejected_with_e1002() {
    let cases = [
        // 1. string to []float64
        r#"
type float_slice = []float64
fn main() {
    _ = float_slice("hello")
}
"#,
        // 2. string to []bool
        r#"
type bool_slice = []bool
fn main() {
    _ = bool_slice("hello")
}
"#,
        // 3. string to array directly
        r#"
type byte_arr = [5]int
fn main() {
    _ = byte_arr("hello")
}
"#,
    ];

    for (idx, src) in cases.iter().enumerate() {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Reject,
            "invalid string-to-container #{idx} should reject, got Accept:\n{src}"
        );
        assert!(
            diags.iter().any(|d| d.code == "HYB-E1002"),
            "invalid string-to-container #{idx} should emit HYB-E1002: {diags:?}\n{src}"
        );
    }
}

#[test]
fn container_conversion_mismatched_element_types_rejected_with_e1002() {
    let cases = [
        // 1. Slice to slice with mismatched element type ([]float64 to []int)
        r#"
type int_slice = []int
fn main() {
    var xs []float64 = []float64{1.0}
    _ = int_slice(xs)
}
"#,
        // 2. Map to map with mismatched value type
        r#"
type int_map = map[string]int
fn main() {
    var m map[string]float64 = map[string]float64{"a": 1.0}
    _ = int_map(m)
}
"#,
        // 3. Chan to chan with mismatched elem type
        r#"
type int_chan = chan int
fn main() {
    ch := make(chan float64, 1)
    _ = int_chan(ch)
}
"#,
        // 4. Array to array with mismatched elem type
        r#"
type int_arr = [2]int
fn main() {
    var arr [2]float64 = [2]float64{1.0, 2.0}
    _ = int_arr(arr)
}
"#,
        // 5. Direct array to slice conversion (Go requires slicing arr[:], not []int(arr))
        r#"
type int_slice = []int
fn main() {
    var arr [2]int = [2]int{1, 2}
    _ = int_slice(arr)
}
"#,
    ];

    for (idx, src) in cases.iter().enumerate() {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Reject,
            "mismatched container conversion #{idx} should reject, got Accept:\n{src}"
        );
        assert!(
            diags.iter().any(|d| d.code == "HYB-E1002"),
            "mismatched container conversion #{idx} should emit HYB-E1002: {diags:?}\n{src}"
        );
    }
}

#[test]
fn valid_conversions_accepted() {
    let src = r#"
type int_slice = []int
type byte_slice = []byte
type int_map = map[string]int
type int_chan = chan int
type int_arr = [2]int

fn main() {
    // 1. byte/rune slice to string. (`[]int` is NOT convertible to `string`
    //    in Go — only `[]byte`/`[]uint8` and `[]rune`/`[]int32` are; see
    //    #2811, which this suite's reject block now covers.)
    bs := []byte{65, 66}
    _ = string(bs)
    rs := []rune{65}
    _ = string(rs)

    // 2. int code point to string
    _ = string(65)

    // 3. string to string
    _ = string("hello")

    // 4. string to byte slice
    var b_slice byte_slice = byte_slice("hello")
    _ = string(b_slice)

    // 5. same element slice alias conversion
    var xs []int = []int{1, 2, 3}
    _ = int_slice(xs)

    // 6. same key-value map alias conversion
    var m map[string]int = map[string]int{"a": 1}
    _ = int_map(m)

    // 7. same element channel alias conversion
    ch := make(chan int, 1)
    _ = int_chan(ch)

    // 8. slice to array with matching element type
    var s []int = []int{1, 2}
    _ = int_arr(s)
}
"#;
    let (verdict, diags) = check(src);
    assert_eq!(
        verdict,
        Verdict::Accept,
        "valid conversions should accept, got Reject: {diags:?}"
    );
}
