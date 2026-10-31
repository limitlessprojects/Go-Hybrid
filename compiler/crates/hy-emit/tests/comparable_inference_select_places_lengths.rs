use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit_src(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parses");
    emit_go(&ast).expect("emits")
}

#[test]
fn generic_comparable_inferred_in_select_places_and_array_lengths() {
    let go = emit_src(
        "\
package main

fn probe_select[k](ch chan<- int, key k) {
    select {
        ch <- len(map[k]int{key: 1}): {}
        default: {}
    }
}

fn probe_place[k](xs []int, key k) {
    xs[len(map[k]int{key: 0}) - 1] = 7
}

fn probe_array_len[k](key k) {
    var arr [len(map[k]int{key: 0})]int
    _ = arr
}

fn main() {}
",
    );

    assert!(
        go.contains("func probe_select[k comparable]("),
        "expected probe_select to infer comparable constraint on k, got:\n{go}"
    );
    assert!(
        go.contains("func probe_place[k comparable]("),
        "expected probe_place to infer comparable constraint on k, got:\n{go}"
    );
    assert!(
        go.contains("func probe_array_len[k comparable]("),
        "expected probe_array_len to infer comparable constraint on k, got:\n{go}"
    );
}
