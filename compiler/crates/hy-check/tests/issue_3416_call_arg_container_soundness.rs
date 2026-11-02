//! issue_3416_call_arg_container_soundness.rs — regression tests for issue #3416:
//! function, method, and func literal calls bypassing container element, key, value, and array scalar checks.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn assert_reject_e1002(src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject verdict");
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "expected HYB-E1002 diagnostic, got: {:?}",
        r.diagnostics.iter().map(|d| (&d.code, &d.message)).collect::<Vec<_>>()
    );
}

#[test]
fn test_fn_arg_chan_scalar_mismatch() {
    let src = r#"
package main

fn consume_chan(ch chan int32) {}

fn main() {
    var bad_ch chan int64
    consume_chan(bad_ch)
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_fn_arg_chan_array_len_mismatch() {
    let src = r#"
package main

fn consume_chan(ch chan [3]int) {}

fn main() {
    var bad_ch chan [4]int
    consume_chan(bad_ch)
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_fn_arg_map_val_scalar_mismatch() {
    let src = r#"
package main

fn consume_map(m map[string]int32) {}

fn main() {
    var bad_m map[string]int64
    consume_map(bad_m)
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_fn_arg_map_key_scalar_mismatch() {
    let src = r#"
package main

fn consume_map(m map[int32]string) {}

fn main() {
    var bad_m map[int64]string
    consume_map(bad_m)
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_fn_arg_map_array_mismatch() {
    let src = r#"
package main

fn consume_map(m map[[3]int]int) {}

fn main() {
    var bad_m map[[4]int]int
    consume_map(bad_m)
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_fn_arg_slice_scalar_mismatch() {
    let src = r#"
package main

fn consume_slice(s []int32) {}

fn main() {
    var bad_sl []int64
    consume_slice(bad_sl)
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_fn_arg_slice_array_mismatch() {
    let src = r#"
package main

fn consume_slice(s [][3]int) {}

fn main() {
    var bad_sl [][4]int
    consume_slice(bad_sl)
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_fn_arg_array_scalar_mismatch() {
    let src = r#"
package main

fn consume_arr(a [3]int32) {}

fn main() {
    var bad_arr [3]int64
    consume_arr(bad_arr)
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_fn_arg_array_alias_mismatch() {
    let src = r#"
package main

type arr32 = [3]int32
type arr64 = [3]int64

fn consume_arr(a arr32) {}

fn main() {
    var bad_arr arr64
    consume_arr(bad_arr)
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_fn_arg_pointer_mismatch() {
    let src = r#"
package main

fn consume_ptr(p *int32) {}

fn main() {
    var bad_p *int64
    consume_ptr(bad_p)
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_method_arg_container_mismatch() {
    let src = r#"
package main

struct worker {}

fn (w worker) do_work(m map[string]int32) {}

fn main() {
    var w worker
    var bad_m map[string]int64
    w.do_work(bad_m)
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_fn_lit_arg_container_mismatch() {
    let src = r#"
package main

fn main() {
    var bad_ch chan int64
    (fn(ch chan int32) {})(bad_ch)
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_fn_arg_chan_identical_accepts() {
    let src = r#"
package main

fn consume_chan(ch chan int32) {}

fn main() {
    var ch chan int32
    consume_chan(ch)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty(), "expected 0 diags, got: {:?}", r.diagnostics);
}

#[test]
fn test_fn_arg_map_identical_accepts() {
    let src = r#"
package main

fn consume_map(m map[string]int32) {}

fn main() {
    var m map[string]int32
    consume_map(m)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty(), "expected 0 diags, got: {:?}", r.diagnostics);
}

#[test]
fn test_fn_arg_slice_identical_accepts() {
    let src = r#"
package main

fn consume_slice(s []int32) {}

fn main() {
    var s []int32
    consume_slice(s)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty(), "expected 0 diags, got: {:?}", r.diagnostics);
}

#[test]
fn test_fn_arg_array_identical_accepts() {
    let src = r#"
package main

type arr32 = [3]int32

fn consume_arr(a arr32) {}

fn main() {
    var a arr32
    consume_arr(a)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty(), "expected 0 diags, got: {:?}", r.diagnostics);
}

#[test]
fn test_fn_arg_pointer_identical_accepts() {
    let src = r#"
package main

fn consume_ptr(p *int32) {}

fn main() {
    var p *int32
    consume_ptr(p)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty(), "expected 0 diags, got: {:?}", r.diagnostics);
}
