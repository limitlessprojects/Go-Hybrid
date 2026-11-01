use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn issue_728_reserved_go_auto_import_ident_is_mangled() {
    let src = "\
fn filepath() string {
    return \"custom\"
}

fn fail() (int, error) {
    return 1, nil
}

fn main() {
    fmt.println(filepath(), fail()?)
}
";
    let ast = parse_file("reserved_go_ident_c.hy", src).expect("parses");
    let go = emit_go(&ast).expect("emits");
    assert!(
        go.contains("func hyUsr_filepath() string"),
        "expected top-level filepath fn to be mangled to hyUsr_filepath, got:\n{}",
        go
    );
    assert!(
        go.contains("filepath.Base(os.Args[0])"),
        "expected E6 main wrapper to use unshadowed filepath.Base, got:\n{}",
        go
    );
}
