use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn issue_728_reserved_go_type_conversion_ident_is_mangled() {
    let src = "\
fn float64(x int) int {
    return x * 100
}

fn main() {
    n := 5
    fmt.println(f64(n) / 2.0, float64(n))
}
";
    let ast = parse_file("reserved_go_ident_a.hy", src).expect("parses");
    let go = emit_go(&ast).expect("emits");
    assert!(
        go.contains("func hyUsr_float64(x int) int"),
        "expected float64 function to be mangled to hyUsr_float64, got:\n{}",
        go
    );
    assert!(
        go.contains("float64(n) / 2.0"),
        "expected f64(n) conversion to emit unshadowed float64(n), got:\n{}",
        go
    );
    assert!(
        go.contains("hyUsr_float64(n)"),
        "expected call to user float64(n) to emit hyUsr_float64(n), got:\n{}",
        go
    );
}
