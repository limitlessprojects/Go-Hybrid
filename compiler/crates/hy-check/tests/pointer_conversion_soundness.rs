use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn test_invalid_mismatched_pointer_conversions_rejected() {
    let src = r#"
type ptr_a = *int32
type ptr_b = *int64
type ptr_fa = *float32
type ptr_fb = *float64

struct point {
    x int64
}

struct node {
    val int64
}

type ptr_point = *point
type ptr_node = *node

type ptr_int = *int64
type ptr_str = *string

fn main() {
    var a ptr_a
    var b ptr_b = ptr_b(a)

    var fa ptr_fa
    var fb ptr_fb = ptr_fb(fa)

    var p ptr_point
    var n ptr_node = ptr_node(p)

    var i ptr_int
    var s ptr_str = ptr_str(i)

    _ = b
    _ = fb
    _ = n
    _ = s
}
"#;
    let ast = parse_file("invalid_ptr_conv.hy", src).expect("snippet parses");
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
fn test_valid_same_base_pointer_conversions_accepted() {
    let src = r#"
type ptr_alias = *int64

struct point {
    x int64
}

type ptr_point_alias = *point

fn main() {
    var p *int64
    var pa ptr_alias = ptr_alias(p)

    var pt *point
    var pta ptr_point_alias = ptr_point_alias(pt)

    _ = pa
    _ = pta
}
"#;
    let ast = parse_file("valid_ptr_conv.hy", src).expect("snippet parses");
    let res = check_file(&ast);
    for d in &res.diagnostics {
        println!("Unexpected Diag: {} - {}", d.code, d.message);
    }
    assert_eq!(res.verdict, Verdict::Accept);
    assert!(res.diagnostics.is_empty());
}
