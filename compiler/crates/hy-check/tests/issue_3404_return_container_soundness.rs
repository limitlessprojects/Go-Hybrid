//! issue_3404_return_container_soundness.rs — regression tests for issue #3404:
//! return statements bypassing container element, key, value, and base type checks
//! for channels, maps, slices, pointers, and array aliases.

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
fn test_return_chan_scalar_mismatch() {
    let src = r#"
package main

fn f() chan int32 {
    var c chan int64
    return c
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_return_chan_array_len_mismatch() {
    let src = r#"
package main

fn f() chan [3]int {
    var c chan [4]int
    return c
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_return_map_val_scalar_mismatch() {
    let src = r#"
package main

fn f() map[string]int32 {
    var m map[string]int64
    return m
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_return_map_key_scalar_mismatch() {
    let src = r#"
package main

fn f() map[int32]string {
    var m map[int64]string
    return m
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_return_map_array_mismatch() {
    let src = r#"
package main

fn f() map[[3]int]int {
    var m map[[4]int]int
    return m
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_return_slice_scalar_mismatch() {
    let src = r#"
package main

fn f() []int32 {
    var s []int64
    return s
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_return_pointer_scalar_mismatch() {
    let src = r#"
package main

fn f() *int32 {
    var p *int64
    return p
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_return_pointer_array_mismatch() {
    let src = r#"
package main

fn f() *[3]int {
    var p *[4]int
    return p
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_return_array_alias_mismatch() {
    let src = r#"
package main

type arr1 = [3]int32
type arr2 = [3]int64

fn f() arr1 {
    var a arr2
    return a
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_return_sized_array_len_mismatch() {
    let src = r#"
package main

fn f() [3]int {
    var a [4]int
    return a
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_return_multi_value_mismatch() {
    let src = r#"
package main

fn f() (chan int32, *int32) {
    var c chan int64
    var p *int32
    return c, p
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_return_chan_identical_accepts() {
    let src = r#"
package main

fn f() chan int32 {
    var c chan int32
    return c
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty(), "expected 0 diags, got: {:?}", r.diagnostics);
}

#[test]
fn test_return_map_identical_accepts() {
    let src = r#"
package main

fn f() map[string]int32 {
    var m map[string]int32
    return m
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty(), "expected 0 diags, got: {:?}", r.diagnostics);
}

#[test]
fn test_return_slice_identical_accepts() {
    let src = r#"
package main

fn f() []int32 {
    var s []int32
    return s
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty(), "expected 0 diags, got: {:?}", r.diagnostics);
}

#[test]
fn test_return_pointer_identical_accepts() {
    let src = r#"
package main

fn f() *int32 {
    var p *int32
    return p
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty(), "expected 0 diags, got: {:?}", r.diagnostics);
}

#[test]
fn test_return_array_alias_identical_accepts() {
    let src = r#"
package main

type arr1 = [3]int32

fn f() arr1 {
    var a arr1
    return a
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty(), "expected 0 diags, got: {:?}", r.diagnostics);
}

#[test]
fn test_return_multi_value_identical_accepts() {
    let src = r#"
package main

fn f() (chan int, map[string]int) {
    var c chan int
    var m map[string]int
    return c, m
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty(), "expected 0 diags, got: {:?}", r.diagnostics);
}

