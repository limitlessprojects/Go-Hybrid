use hy_emit::emit_go as emit;
use hy_syntax::parse_file;

#[test]
fn emit_bytes_buffer_and_strings_builder_on_user_struct_fields_and_try_arity() {
    let src = r#"
import "bytes"
import "strings"

struct holder {
    buf bytes.buffer
    sb strings.builder
}

fn run(h *holder) error {
    h.buf.grow(64)
    h.buf.write_byte(65)?
    n := h.buf.write_string("hello")?
    h.buf.unread_byte()?
    s := h.buf.string()
    h.sb.grow(32)
    h.sb.write_byte(66)?
    m := h.sb.write_string(s)?
    _ := n + m
    return nil
}

fn main() {
    var h holder
    _ := run(&h)
}
"#;
    let ast = parse_file("t.hy", src).expect("parse ok");
    let go = emit(&ast).expect("emit ok");
    assert!(go.contains("h.buf.Grow(64)"), "got:\n{go}");
    assert!(go.contains("h.buf.WriteByte(65)"), "got:\n{go}");
    assert!(go.contains("h.buf.WriteString(\"hello\")"), "got:\n{go}");
    assert!(go.contains("h.buf.UnreadByte()"), "got:\n{go}");
    assert!(go.contains("h.buf.String()"), "got:\n{go}");
    assert!(go.contains("h.sb.Grow(32)"), "got:\n{go}");
    assert!(go.contains("h.sb.WriteByte(66)"), "got:\n{go}");
    assert!(go.contains("h.sb.WriteString(s)"), "got:\n{go}");
}

#[test]
fn emit_sync_waitgroup_spelling_on_user_struct_field() {
    let src = r#"
import "sync"

struct pool {
    wg sync.waitgroup
}

fn main() {
    var p pool
    p.wg.add(1)
    p.wg.done()
    p.wg.wait()
}
"#;
    let ast = parse_file("t.hy", src).expect("parse ok");
    let go = emit(&ast).expect("emit ok");
    assert!(go.contains("p.wg.Add(1)"), "got:\n{go}");
    assert!(go.contains("p.wg.Done()"), "got:\n{go}");
    assert!(go.contains("p.wg.Wait()"), "got:\n{go}");
}
