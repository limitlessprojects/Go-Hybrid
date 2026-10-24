use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn channel_receives_and_field_channels_propagate_element_binding_metadata() {
    let src = "\
import \"os\"

struct holder { ch chan int }
struct node { v int }
fn take_ptr(n *node) { _ = n.v }

fn run(outer chan chan int, files chan *os.file, nodes chan *node, h holder) {
    c1 := <-outer
    for x in c1 { _ = x }
    c2, ok := <-outer
    if ok {
        for y in c2 { _ = y }
    }
    select {
        c3 := <-outer: {
            for z in c3 { _ = z }
        }
        f := <-files: {
            _ = f.close()
        }
        n := <-nodes: {
            take_ptr(n)
        }
        default: {}
    }
    for w in <-outer { _ = w }
    for q in h.ch { _ = q }
}

fn main() {}
";
    let file = parse_file("test.hy", src).expect("parse ok");
    let go = emit_go(&file).expect("emit ok");
    assert!(go.contains("for x := range c1"), "c1 must range as channel:\n{go}");
    assert!(go.contains("for y := range c2"), "c2 must range as channel:\n{go}");
    assert!(go.contains("for z := range c3"), "c3 must range as channel:\n{go}");
    assert!(go.contains("f.Close()"), "foreign elem method must normalize:\n{go}");
    assert!(go.contains("take_ptr(n)"), "pointer elem must not double-address:\n{go}");
    assert!(go.contains("for w := range <-outer"), "receive expr must range as channel:\n{go}");
    assert!(go.contains("for q := range h.ch"), "struct field chan must range as channel:\n{go}");
}

#[test]
fn slice_channel_element_keeps_two_var_value_range_lowering() {
    let src = "\
fn run(batches chan []int) {
    xs := <-batches
    for x in xs { _ = x }
}
fn main() {}
";
    let file = parse_file("test.hy", src).expect("parse ok");
    let go = emit_go(&file).expect("emit ok");
    assert!(go.contains("for _, x := range xs"), "slice elem keeps 2-var range:\n{go}");
}
