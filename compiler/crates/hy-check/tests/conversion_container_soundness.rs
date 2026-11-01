use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> (Verdict, Vec<hy_diag::Diagnostic>) {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    (res.verdict, res.diagnostics)
}

#[test]
fn container_conversion_mismatched_types_rejected() {
    let cases = [
        // 1. Slice to slice with mismatched element type
        r#"
type int_slice = []int
fn main() {
    var xs []float64 = []float64{1.0}
    _ = int_slice(xs)
}
"#,
        // 2. String to non-int slice
        r#"
type float_slice = []float64
fn main() {
    _ = float_slice("hello")
}
"#,
        // 3. Non-int slice to string
        r#"
fn main() {
    var xs []float64 = []float64{1.0}
    _ = string(xs)
}
"#,
        // 4. Map to map with mismatched value type
        r#"
type int_map = map[string]int
fn main() {
    var m map[string]float64 = map[string]float64{"a": 1.0}
    _ = int_map(m)
}
"#,
        // 5. Chan to chan with mismatched elem type
        r#"
type int_chan = chan int
fn main() {
    ch := make(chan float64, 1)
    _ = int_chan(ch)
}
"#,
    ];

    for src in cases {
        let (verdict, diags) = check(src);
        assert_eq!(
            verdict,
            Verdict::Reject,
            "expected reject for mismatched container conversion:\n{src}"
        );
        assert!(
            diags.iter().any(|d| d.code == "HYB-E1002"),
            "expected HYB-E1002 in diags: {:?}",
            diags
        );
    }
}

#[test]
fn container_conversion_compatible_types_accepted() {
    let src = r#"
type int_slice = []int
type byte_slice = []byte
type int_map = map[string]int
type int_chan = chan int

fn main() {
    // 1. Same element slice alias conversion
    var xs []int = []int{1, 2, 3}
    _ = int_slice(xs)

    // 2. String and []byte mutual conversions
    var bs byte_slice = byte_slice("hello")
    _ = string(bs)

    // 3. Same key-value map alias conversion
    var m map[string]int = map[string]int{"a": 1}
    _ = int_map(m)

    // 4. Same element channel alias conversion
    ch := make(chan int, 1)
    _ = int_chan(ch)
}
"#;
    let (verdict, diags) = check(src);
    assert_eq!(
        verdict,
        Verdict::Accept,
        "expected accept for valid container conversions: {:?}",
        diags
    );
}
