use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit_src(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parses");
    emit_go(&ast).expect("emits")
}

#[test]
fn test_issue_977_defect_a_method_callee_auto_address() {
    let src = r#"
struct inner {
    val int
}

struct box {
    x int
}

fn (b *box) take(v *inner) {
    _ = v
}

fn main() {
    b := box{}
    v := inner{val: 42}
    b.take(v)
}
"#;
    let go = emit_src(src);
    assert!(
        go.contains("b.take(&v)"),
        "expected b.take(&v), got:\n{go}"
    );
}

#[test]
fn test_issue_977_defect_b_index_already_pointer_no_double_address() {
    let src = r#"
struct inner {
    val int
}

fn take(v *inner) {
    _ = v
}

fn main() {
    q := []*inner{new(inner)}
    take(q[0])
}
"#;
    let go = emit_src(src);
    assert!(
        go.contains("take(q[0])") && !go.contains("take(&q[0])"),
        "expected take(q[0]) without &, got:\n{go}"
    );
}

#[test]
fn test_issue_977_method_callee_with_pointer_slice_index() {
    let src = r#"
struct inner {
    val int
}

struct box {
    x int
}

fn (b *box) take(v *inner) {
    _ = v
}

fn main() {
    b := box{}
    q := []*inner{new(inner)}
    b.take(q[0])
}
"#;
    let go = emit_src(src);
    assert!(
        go.contains("b.take(q[0])") && !go.contains("b.take(&q[0])"),
        "expected b.take(q[0]) without &, got:\n{go}"
    );
}

#[test]
fn test_issue_977_var_decl_slice_and_map_pointer_elem() {
    let src = r#"
struct inner {
    val int
}

fn take(v *inner) {
    _ = v
}

fn main() {
    var s []*inner
    take(s[0])
    var m map[string]*inner
    take(m["k"])
}
"#;
    let go = emit_src(src);
    assert!(
        go.contains("take(s[0])") && !go.contains("take(&s[0])"),
        "expected take(s[0]) without &, got:\n{go}"
    );
    assert!(
        go.contains("take(m[\"k\"])") && !go.contains("take(&m[\"k\"])"),
        "expected take(m[\"k\"]) without &, got:\n{go}"
    );
}

#[test]
fn test_issue_977_struct_pointer_field_argument() {
    let src = r#"
struct inner {
    val int
}

struct holder {
    item *inner
}

fn take(v *inner) {
    _ = v
}

fn main() {
    h := holder{item: new(inner)}
    take(h.item)
}
"#;
    let go = emit_src(src);
    assert!(
        go.contains("take(h.item)") && !go.contains("take(&h.item)"),
        "expected take(h.item) without &, got:\n{go}"
    );
}
