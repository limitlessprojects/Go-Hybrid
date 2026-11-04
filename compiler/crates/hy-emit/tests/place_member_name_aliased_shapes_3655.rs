use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("test source must parse");
    emit_go(&ast).expect("test source must emit cleanly")
}

#[test]
fn place_member_name_and_rebindings_keep_shapes_behind_type_aliases_3655() {
    let src = r#"import "net/url"

type my_url = url.url
struct leaf {
    u my_url
}
type wrap = leaf
struct holder {
    sub wrap
}

fn run(h holder) {
    h.sub.u.raw_query = "a=1"
    s := h.sub
    s.u.raw_query = "b=2"
}
"#;
    let go = emit(src);
    assert!(
        go.contains("h.sub.u.RawQuery = "a=1""),
        "expected RawQuery on h.sub.u, got:
{go}"
    );
    assert!(
        go.contains("s.u.RawQuery = "b=2""),
        "expected RawQuery on s.u, got:
{go}"
    );
}
