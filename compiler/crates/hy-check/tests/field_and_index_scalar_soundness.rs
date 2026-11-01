use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn test_invalid_field_and_index_assignments_rejected() {
    let src = r#"
struct point {
    x int64
    f float64
}

fn main() {
    var p point
    var a32 int32 = 42
    var f32 float32 = 3.14

    // Struct field assignment scalar mismatch
    p.x = a32
    p.f = f32

    // Slice index assignment scalar mismatch
    var xs []int64 = []int64{ 1, 2, 3 }
    xs[0] = a32

    // Array index assignment scalar mismatch
    var arr [4]int64
    arr[0] = a32

    // Parallel assignment with field and index target mismatch
    p.x, xs[1] = a32, a32

    _ = p
    _ = xs
    _ = arr
}
"#;
    let ast = parse_file("invalid_field_index.hy", src).expect("snippet parses");
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
fn test_valid_field_and_index_assignments_accepted() {
    let src = r#"
struct point {
    x int64
    f float64
}

fn main() {
    var p point
    var b64 int64 = 100
    var g64 float64 = 2.718

    // Same-type field assignments
    p.x = b64
    p.f = g64

    // Same-type slice and array assignments
    var xs []int64 = []int64{ 1, 2, 3 }
    xs[0] = b64

    var arr [4]int64
    arr[0] = b64

    // Untyped constants into fields and indices
    p.x = 42
    p.f = 3.14
    xs[1] = 99
    arr[1] = 88

    _ = p
    _ = xs
    _ = arr
}
"#;
    let ast = parse_file("valid_field_index.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    for d in &res.diagnostics {
        println!("Unexpected Diag: {} - {}", d.code, d.message);
    }
    assert_eq!(res.verdict, Verdict::Accept);
    assert!(res.diagnostics.is_empty());
}
