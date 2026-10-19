use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn issue_728_reserved_go_local_type_name_is_mangled() {
    let src = "\
fn main() {
    int64 := 7
    var x i64 = 10
    fmt.println(int64, x)
}
";
    let ast = parse_file("reserved_go_ident_b.hy", src).expect("parses");
    let go = emit_go(&ast).expect("emits");
    assert!(
        go.contains("hyUsr_int64 := 7"),
        "expected local int64 binding to be mangled to hyUsr_int64, got:\n{}",
        go
    );
    assert!(
        go.contains("var x int64 = 10"),
        "expected i64 type annotation to emit unshadowed int64, got:\n{}",
        go
    );
}
