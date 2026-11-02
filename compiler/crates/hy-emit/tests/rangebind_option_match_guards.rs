use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn range_binder_read_in_guarded_none_or_fallback_wildcard_is_preserved() {
    let src = "package main\nimport \"fmt\"\nfn main() {\n xs := []int{1}\n var opt option[int]\n for i, v in xs {\n _ = i\n match opt {\n some(_) => { fmt.println(\"s\") }\n none if v == 1 => { fmt.println(\"n\") }\n _ => { fmt.println(v) }\n }\n }\n}\n";
    let go_src = emit_go(&parse_file("test.hy", src).expect("parse")).expect("emit");
    assert!(go_src.contains(", v := range xs"), "expected `v` preserved in range header, got:\n{}", go_src);
}
