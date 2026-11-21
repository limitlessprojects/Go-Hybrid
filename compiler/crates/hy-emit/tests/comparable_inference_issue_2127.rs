//! comparable_inference_issue_2127.rs
//! Regression test for issue #2127:
//! hy-emit generic comparable inference skips select arm headers,
//! assign target places, and array lengths.

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit_src(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parses");
    emit_go(&ast).expect("emits")
}

#[test]
fn test_comparable_inference_in_select_arm_send_and_recv() {
    let src = r#"
package main

fn probe_select_send[k](ch chan<- int, key k) {
    select {
        ch <- len(map[k]int{key: 1}): {}
        default: {}
    }
}

fn probe_select_recv[k](ch chan map[k]int) {
    select {
        m := <-ch: {
            _ = m
        }
        default: {}
    }
}
"#;
    let go = emit_src(src);
    assert!(
        go.contains("func probe_select_send[k comparable]("),
        "expected comparable constraint for probe_select_send, got:
{go}"
    );
}

#[test]
fn test_comparable_inference_in_assign_target_places() {
    let src = r#"
package main

fn probe_place_index[k](xs []int, key k) {
    xs[len(map[k]int{key: 0}) - 1] = 7
}
"#;
    let go = emit_src(src);
    assert!(
        go.contains("func probe_place_index[k comparable]("),
        "expected comparable constraint for probe_place_index, got:
{go}"
    );
}

#[test]
fn test_comparable_inference_in_array_type_expr_length() {
    let src = r#"
package main

fn probe_array_len[k](key k) {
    var arr [len(map[k]int{key: 0})]int
    _ = arr
}

struct box_array[k] {
    items [len(map[k]int{})]int
}
"#;
    let go = emit_src(src);
    assert!(
        go.contains("func probe_array_len[k comparable]("),
        "expected comparable constraint for probe_array_len, got:
{go}"
    );
    assert!(
        go.contains("type box_array[k comparable] struct {"),
        "expected comparable constraint for box_array struct, got:
{go}"
    );
}
