//! issue_3405_struct_literal_container_soundness.rs — regression tests for issue #3405:
//! struct literal fields bypassing container element, key, value, and array scalar checks.

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
fn test_struct_lit_chan_scalar_mismatch() {
    let src = r#"
package main

struct pair {
    ch chan int32
}

fn sink(p pair) {}

fn main() {
    var bad_ch chan int64
    let p = pair{ ch: bad_ch }
    sink(p)
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_struct_lit_chan_array_len_mismatch() {
    let src = r#"
package main

struct pair {
    ch chan [3]int
}

fn sink(p pair) {}

fn main() {
    var bad_ch chan [4]int
    let p = pair{ ch: bad_ch }
    sink(p)
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_struct_lit_map_val_scalar_mismatch() {
    let src = r#"
package main

struct pair {
    m map[string]int32
}

fn sink(p pair) {}

fn main() {
    var bad_m map[string]int64
    let p = pair{ m: bad_m }
    sink(p)
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_struct_lit_map_key_scalar_mismatch() {
    let src = r#"
package main

struct pair {
    m map[int32]string
}

fn sink(p pair) {}

fn main() {
    var bad_m map[int64]string
    let p = pair{ m: bad_m }
    sink(p)
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_struct_lit_map_array_mismatch() {
    let src = r#"
package main

struct pair {
    m map[[3]int]int
}

fn sink(p pair) {}

fn main() {
    var bad_m map[[4]int]int
    let p = pair{ m: bad_m }
    sink(p)
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_struct_lit_slice_scalar_mismatch() {
    let src = r#"
package main

struct pair {
    sl []int32
}

fn sink(p pair) {}

fn main() {
    var bad_sl []int64
    let p = pair{ sl: bad_sl }
    sink(p)
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_struct_lit_slice_array_mismatch() {
    let src = r#"
package main

struct pair {
    sl [][3]int
}

fn sink(p pair) {}

fn main() {
    var bad_sl [][4]int
    let p = pair{ sl: bad_sl }
    sink(p)
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_struct_lit_array_scalar_mismatch() {
    let src = r#"
package main

struct pair {
    arr [3]int32
}

fn sink(p pair) {}

fn main() {
    var bad_arr [3]int64
    let p = pair{ arr: bad_arr }
    sink(p)
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_struct_lit_array_alias_mismatch() {
    let src = r#"
package main

type arr1 = [3]int32
type arr2 = [3]int64

struct pair {
    arr arr1
}

fn sink(p pair) {}

fn main() {
    var bad_arr arr2
    let p = pair{ arr: bad_arr }
    sink(p)
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_struct_lit_chan_identical_accepts() {
    let src = r#"
package main

struct pair {
    ch chan int32
}

fn sink(p pair) {}

fn main() {
    var ch chan int32
    let p = pair{ ch: ch }
    sink(p)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty(), "expected 0 diags, got: {:?}", r.diagnostics);
}

#[test]
fn test_struct_lit_map_identical_accepts() {
    let src = r#"
package main

struct pair {
    m map[string]int32
}

fn sink(p pair) {}

fn main() {
    var m map[string]int32
    let p = pair{ m: m }
    sink(p)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty(), "expected 0 diags, got: {:?}", r.diagnostics);
}

#[test]
fn test_struct_lit_slice_identical_accepts() {
    let src = r#"
package main

struct pair {
    sl []int32
}

fn sink(p pair) {}

fn main() {
    var sl []int32
    let p = pair{ sl: sl }
    sink(p)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty(), "expected 0 diags, got: {:?}", r.diagnostics);
}

#[test]
fn test_struct_lit_array_identical_accepts() {
    let src = r#"
package main

type arr1 = [3]int32

struct pair {
    arr arr1
}

fn sink(p pair) {}

fn main() {
    var arr arr1
    let p = pair{ arr: arr }
    sink(p)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty(), "expected 0 diags, got: {:?}", r.diagnostics);
}

#[test]
fn test_struct_lit_all_valid_accepts() {
    let src = r#"
package main

type arr32 = [3]int32

struct pair {
    ch chan int32
    ch_arr chan [3]int
    m_val map[string]int32
    m_key map[int32]string
    m_arr map[[3]int]int
    sl []int32
    sl_arr [][3]int
    arr [3]int32
    arr_alias arr32
}

fn sink(p pair) {}

fn main() {
    var ch chan int32
    var ch_arr chan [3]int
    var m_val map[string]int32
    var m_key map[int32]string
    var m_arr map[[3]int]int
    var sl []int32
    var sl_arr [][3]int
    var arr [3]int32
    var arr_alias arr32

    let p = pair{
        ch: ch,
        ch_arr: ch_arr,
        m_val: m_val,
        m_key: m_key,
        m_arr: m_arr,
        sl: sl,
        sl_arr: sl_arr,
        arr: arr,
        arr_alias: arr_alias,
    }
    sink(p)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty(), "expected 0 diags, got: {:?}", r.diagnostics);
}
