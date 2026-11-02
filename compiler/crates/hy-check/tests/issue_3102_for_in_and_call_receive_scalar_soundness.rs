//! Regression test for issue #3102:
//! hy-check: typed widths from for-in bindings (slice/array/map/channel), method calls,
//! fn-literal calls and channel receives escape the #2712 width check (HYB-E1002).
//!
//! Under Go Spec and GUARANTEES.md Row 13 (BUILDABLE), values produced by:
//! 1. for-in loop bindings over typed slices, arrays, maps, and channels
//! 2. Channel receive expressions (`<-c`)
//! 3. Method calls (`a.g()`)
//! 4. Calls to local function-literal bindings (`f()`)
//! must preserve their declared scalar widths and array dimensions when assigned or passed as arguments.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("snippet should parse");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

// 1. Slice element scalar mismatch in var decl initialization
#[test]
fn test_for_in_slice_elem_scalar_mismatch() {
    let src = r#"
package main
fn main() {
    xs := []int8{1, 2}
    for x in xs {
        var y int64 = x
        _ = y
    }
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 2. Sized array element scalar mismatch with index variable
#[test]
fn test_for_in_array_elem_scalar_mismatch() {
    let src = r#"
package main
fn main() {
    xs := [3]int16{1, 2, 3}
    for i, x in xs {
        var y int = x
        _ = y
    }
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 3. Float slice element scalar mismatch (float32 vs float64)
#[test]
fn test_for_in_float_slice_elem_scalar_mismatch() {
    let src = r#"
package main
fn main() {
    xs := []float32{1.5}
    for x in xs {
        var y float64 = x
        _ = y
    }
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 4. Map value scalar mismatch
#[test]
fn test_for_in_map_val_scalar_mismatch() {
    let src = r#"
package main
fn main() {
    m := map[string]int8{"a": 1}
    for k, v in m {
        var y int64 = v
        _ = y
    }
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 5. Map key scalar mismatch
#[test]
fn test_for_in_map_key_scalar_mismatch() {
    let src = r#"
package main
fn main() {
    m := map[int8]string{1: "a"}
    for k, v in m {
        var y int64 = k
        _ = y
    }
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 6. Slice element argument scalar mismatch in function call
#[test]
fn test_for_in_slice_elem_arg_scalar_mismatch() {
    let src = r#"
package main
fn take(y int64) {}
fn main() {
    for x in []int8{1} {
        take(x)
    }
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 7. Slice element arithmetic scalar mismatch
#[test]
fn test_for_in_slice_elem_arith_scalar_mismatch() {
    let src = r#"
package main
fn main() {
    xs := []int8{1, 2}
    var t int64 = 0
    for x in xs {
        t = t + x
    }
    _ = t
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 8. Method call result scalar mismatch
#[test]
fn test_method_call_result_scalar_mismatch() {
    let src = r#"
package main
struct p { x int }
fn (q p) g() int8 { return 1 }
fn main() {
    a := p{x: 1}
    var y int64 = a.g()
    _ = y
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 9. Local fn-literal call result scalar mismatch
#[test]
fn test_fn_literal_local_call_result_scalar_mismatch() {
    let src = r#"
package main
fn main() {
    f := fn() int8 { return 1 }
    var y int64 = f()
    _ = y
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 10. Channel receive scalar mismatch in var decl initialization
#[test]
fn test_chan_receive_scalar_mismatch() {
    let src = r#"
package main
fn main() {
    c := make(chan int8, 1)
    var y int64 = <-c
    _ = y
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 11. Channel receive scalar mismatch through short-decl local
#[test]
fn test_chan_receive_binding_scalar_mismatch() {
    let src = r#"
package main
fn main() {
    c := make(chan uint16, 1)
    x := <-c
    var y int = x
    _ = y
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 12. Channel for-in element scalar mismatch
#[test]
fn test_for_in_chan_elem_scalar_mismatch() {
    let src = r#"
package main
fn main() {
    c := make(chan int64, 1)
    for v in c {
        var y int32 = v
        _ = y
    }
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 13. Sized array dimension mismatch on for-in element
#[test]
fn test_for_in_sized_array_dimension_mismatch() {
    let src = r#"
package main
fn main() {
    xs := [][2]int{[2]int{1, 2}}
    for _, v in xs {
        var y [3]int = v
        _ = y
    }
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 14. Nested slice element scalar mismatch on for-in element
#[test]
fn test_for_in_nested_slice_scalar_mismatch() {
    let src = r#"
package main
fn main() {
    xs := [][]int64{[]int64{1}}
    for _, v in xs {
        var y []int32 = v
        _ = y
    }
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 15. Type alias resolution on for-in loop element
#[test]
fn test_for_in_type_alias_scalar_mismatch() {
    let src = r#"
package main
type my_int = int64
fn main() {
    xs := []my_int{1}
    for _, v in xs {
        var y int32 = v
        _ = y
    }
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

// 16. Matching scalar widths and dimensions accepted
#[test]
fn test_matching_widths_and_dimensions_accepted() {
    let src = r#"
package main
struct p { x int }
fn (q p) g() int8 { return 1 }
fn main() {
    xs := []int8{1, 2}
    var t int8 = 0
    for x in xs {
        t = t + x
    }
    m := map[int8]float32{1: 1.5}
    for k, v in m {
        var a int8 = k
        var b float32 = v
        _ = a
        _ = b
    }
    a := p{x: 1}
    var g int8 = a.g()
    _ = g
    f := fn() int8 { return 1 }
    var h int8 = f()
    _ = h
    c := make(chan int8, 1)
    var r int8 = <-c
    _ = r
    var arr [2]int = [2]int{1, 2}
    matrix := [][2]int{arr}
    for _, row in matrix {
        var copied [2]int = row
        _ = copied
    }
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty(), "Expected 0 diagnostics, got: {:?}", r.diagnostics);
}
