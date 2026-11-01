use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn test_typed_integer_reads_mismatch_rejected_with_e1002() {
    let src = r#"
struct pt {
    v i64
}
fn main() {
    s := []i64{1}
    m := map[string]i64{"a": 1}
    p := pt{v: 1}
    var a int = s[0]
    var b int = m["a"]
    var c int = p.v
    x := s[0]
    var d int = x
    str := "abc"
    var r rune = str[0]
    fmt.println(a, b, c, d, r)
}
"#;
    let file = parse_file("test.hy", src).expect("failed to parse");
    let result = check_file(&file);
    assert_eq!(result.verdict, Verdict::Reject, "expected reject, got accept");
    let e1002_count = result.diagnostics.iter().filter(|d| d.code == "HYB-E1002").count();
    assert!(e1002_count >= 5, "expected at least 5 HYB-E1002 errors, found {e1002_count}: {:?}", result.diagnostics);
}

#[test]
fn test_matching_typed_integer_reads_accepted() {
    let src = r#"
struct pt {
    v i64
}
fn main() {
    s := []i64{1}
    m := map[string]i64{"a": 1}
    p := pt{v: 1}
    var a i64 = s[0]
    var b i64 = m["a"]
    var c i64 = p.v
    x := s[0]
    var d i64 = x
    str := "abc"
    var b0 byte = str[0]
    var u0 uint8 = str[0]
    fmt.println(a, b, c, d, b0, u0)
}
"#;
    let file = parse_file("test_ok.hy", src).expect("failed to parse");
    let result = check_file(&file);
    assert_eq!(result.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", result.diagnostics);
}

#[test]
fn test_assignment_mismatch_to_field_and_index_rejected_e1002() {
    let src = r#"
struct point {
    x int64
    f float64
}

fn main() {
    var p point
    var a32 int32 = 42
    var f32 float32 = 3.14

    p.x = a32
    p.f = f32

    var xs []int64 = []int64{ 1, 2, 3 }
    xs[0] = a32

    _ = p
    _ = xs
}
"#;
    let file = parse_file("test_assign_mismatch.hy", src).expect("failed to parse");
    let result = check_file(&file);
    assert_eq!(result.verdict, Verdict::Reject, "expected reject, got accept");
    let e1002_count = result.diagnostics.iter().filter(|d| d.code == "HYB-E1002").count();
    assert!(e1002_count >= 3, "expected at least 3 HYB-E1002 errors, found {e1002_count}: {:?}", result.diagnostics);
}

#[test]
fn test_assignment_matching_to_field_and_index_accepted() {
    let src = r#"
struct point {
    x int64
    f float64
}

fn main() {
    var p point
    var a64 int64 = 42
    var f64_val float64 = 3.14

    p.x = a64
    p.f = f64_val

    var xs []int64 = []int64{ 1, 2, 3 }
    xs[0] = a64

    _ = p
    _ = xs
}
"#;
    let file = parse_file("test_assign_ok.hy", src).expect("failed to parse");
    let result = check_file(&file);
    assert_eq!(result.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", result.diagnostics);
}
