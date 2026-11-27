//! issue_3448_var_decl_container_soundness.rs — regression tests for issue #3448:
//! variable declarations (`var` and `let`) bypassing type checks for channels,
//! maps, slices, pointers, and array aliases with mismatched element, key, or value types.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn assert_rejected(name: &str, src: &str) {
    let file = parse_file("test.hy", src).expect("parse failed");
    let verdict = check_file(&file);
    let e1002_count = verdict
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error && d.code == "HYB-E1002")
        .count();
    assert!(
        e1002_count > 0 && verdict.verdict == Verdict::Reject,
        "[{name}] expected rejection with HYB-E1002, got verdict {:?}, diags: {:?}",
        verdict.verdict,
        verdict.diagnostics
    );
}

fn assert_accepted(name: &str, src: &str) {
    let file = parse_file("test.hy", src).expect("parse failed");
    let verdict = check_file(&file);
    let errors: Vec<_> = verdict
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert!(
        errors.is_empty() && verdict.verdict == Verdict::Accept,
        "[{name}] expected accept with 0 errors, got verdict {:?}, errors: {:?}",
        verdict.verdict,
        errors
    );
}

#[test]
fn test_var_decl_chan_element_mismatch() {
    let src = r#"
package main
fn consume(c chan int32) {}
fn test(ch64 chan int64) {
    var c chan int32 = ch64
    consume(c)
}
fn main() {}
"#;
    assert_rejected("var_decl_chan_element_mismatch", src);
}

#[test]
fn test_var_decl_map_value_mismatch() {
    let src = r#"
package main
fn consume(m map[string]int32) {}
fn test(m64 map[string]int64) {
    var m map[string]int32 = m64
    consume(m)
}
fn main() {}
"#;
    assert_rejected("var_decl_map_value_mismatch", src);
}

#[test]
fn test_var_decl_map_key_mismatch() {
    let src = r#"
package main
fn consume(m map[int32]string) {}
fn test(m64 map[int64]string) {
    var m map[int32]string = m64
    consume(m)
}
fn main() {}
"#;
    assert_rejected("var_decl_map_key_mismatch", src);
}

#[test]
fn test_var_decl_slice_element_mismatch() {
    let src = r#"
package main
fn consume(s []int32) {}
fn test(s64 []int64) {
    var s []int32 = s64
    consume(s)
}
fn main() {}
"#;
    assert_rejected("var_decl_slice_element_mismatch", src);
}

#[test]
fn test_var_decl_pointer_base_mismatch() {
    let src = r#"
package main
fn consume(p *int32) {}
fn test(p64 *int64) {
    var p *int32 = p64
    consume(p)
}
fn main() {}
"#;
    assert_rejected("var_decl_pointer_base_mismatch", src);
}

#[test]
fn test_var_decl_array_alias_mismatch() {
    let src = r#"
package main
type arr32 = [32]byte
type arr64 = [64]byte
fn consume(a arr32) {}
fn test(a64 arr64) {
    var a arr32 = a64
    consume(a)
}
fn main() {}
"#;
    assert_rejected("var_decl_array_alias_mismatch", src);
}

#[test]
fn test_var_decl_nested_container_mismatch() {
    let src_nested_slice = r#"
package main
fn consume(s [][]int32) {}
fn test(s_bad [][]int64) {
    var s [][]int32 = s_bad
    consume(s)
}
fn main() {}
"#;
    assert_rejected("var_decl_nested_slice_mismatch", src_nested_slice);

    let src_chan_map = r#"
package main
fn consume(m map[string]chan int32) {}
fn test(m_bad map[string]chan int64) {
    var m map[string]chan int32 = m_bad
    consume(m)
}
fn main() {}
"#;
    assert_rejected("var_decl_chan_map_mismatch", src_chan_map);
}

#[test]
fn test_let_decl_container_mismatches() {
    let src_chan = r#"
package main
fn consume(c chan int32) {}
fn test(ch64 chan int64) {
    let c chan int32 = ch64
    consume(c)
}
fn main() {}
"#;
    assert_rejected("let_decl_chan_mismatch", src_chan);

    let src_map = r#"
package main
fn consume(m map[string]int32) {}
fn test(m64 map[string]int64) {
    let m map[string]int32 = m64
    consume(m)
}
fn main() {}
"#;
    assert_rejected("let_decl_map_mismatch", src_map);

    let src_slice = r#"
package main
fn consume(s []int32) {}
fn test(s64 []int64) {
    let s []int32 = s64
    consume(s)
}
fn main() {}
"#;
    assert_rejected("let_decl_slice_mismatch", src_slice);

    let src_ptr = r#"
package main
fn consume(p *int32) {}
fn test(p64 *int64) {
    let p *int32 = p64
    consume(p)
}
fn main() {}
"#;
    assert_rejected("let_decl_ptr_mismatch", src_ptr);

    let src_arr = r#"
package main
type arr32 = [32]byte
type arr64 = [64]byte
fn consume(a arr32) {}
fn test(a64 arr64) {
    let a arr32 = a64
    consume(a)
}
fn main() {}
"#;
    assert_rejected("let_decl_arr_mismatch", src_arr);
}

#[test]
fn test_var_decl_containers_valid_controls() {
    let src = r#"
package main
type arr32 = [32]byte

fn consume_chan(c chan int32) {}
fn consume_map(m map[string]int32) {}
fn consume_slice(s []int32) {}
fn consume_ptr(p *int32) {}
fn consume_arr(a arr32) {}

fn test_valid(
    ch chan int32,
    m map[string]int32,
    s []int32,
    p *int32,
    a arr32,
) {
    var v_ch chan int32 = ch
    let l_ch chan int32 = ch
    consume_chan(v_ch)
    consume_chan(l_ch)

    var v_m map[string]int32 = m
    let l_m map[string]int32 = m
    consume_map(v_m)
    consume_map(l_m)

    var v_s []int32 = s
    let l_s []int32 = s
    consume_slice(v_s)
    consume_slice(l_s)

    var v_p *int32 = p
    let l_p *int32 = p
    consume_ptr(v_p)
    consume_ptr(l_p)

    var v_a arr32 = a
    let l_a arr32 = a
    consume_arr(v_a)
    consume_arr(l_a)
}

fn main() {}
"#;
    assert_accepted("var_decl_containers_valid_controls", src);
}
