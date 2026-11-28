//! issue_3398_assignment_container_soundness.rs — regression tests for issue #3398:
//! single and parallel assignment statements (`=`) bypassing container element, key,
//! value, and base type checks for channels, maps, slices, pointers, and array aliases.

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
fn test_assign_chan_scalar_mismatch() {
    let src = r#"
package main

fn sink(c chan int32) {}

fn main() {
    var c1 chan int32
    var c2 chan int64
    c1 = c2
    sink(c1)
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_assign_chan_array_len_mismatch() {
    let src = r#"
package main

fn sink(c chan [3]int) {}

fn main() {
    var c1 chan [3]int
    var c2 chan [4]int
    c1 = c2
    sink(c1)
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_assign_map_val_scalar_mismatch() {
    let src = r#"
package main

fn sink(m map[string]int32) {}

fn main() {
    var m1 map[string]int32
    var m2 map[string]int64
    m1 = m2
    sink(m1)
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_assign_map_key_scalar_mismatch() {
    let src = r#"
package main

fn sink(m map[int32]string) {}

fn main() {
    var m1 map[int32]string
    var m2 map[int64]string
    m1 = m2
    sink(m1)
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_assign_map_array_mismatch() {
    let src = r#"
package main

fn sink(m map[[3]int]int) {}

fn main() {
    var m1 map[[3]int]int
    var m2 map[[4]int]int
    m1 = m2
    sink(m1)
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_assign_slice_scalar_mismatch() {
    let src = r#"
package main

fn sink(s []int32) {}

fn main() {
    var s1 []int32
    var s2 []int64
    s1 = s2
    sink(s1)
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_assign_pointer_scalar_mismatch() {
    let src = r#"
package main

fn sink(p *int32) {}

fn main() {
    var p1 *int32
    var p2 *int64
    p1 = p2
    sink(p1)
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_assign_pointer_array_mismatch() {
    let src = r#"
package main

fn sink(p *[3]int) {}

fn main() {
    var p1 *[3]int
    var p2 *[4]int
    p1 = p2
    sink(p1)
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_assign_array_alias_mismatch() {
    let src = r#"
package main

type arr1 = [3]int32
type arr2 = [3]int64

fn sink(a arr1) {}

fn main() {
    var a1 arr1
    var a2 arr2
    a1 = a2
    sink(a1)
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_parallel_assign_chan_mismatch() {
    let src = r#"
package main

fn sink(c chan int32, x int) {}

fn main() {
    var c1 chan int32
    var c2 chan int64
    var a int
    var b int
    c1, a = c2, b
    sink(c1, a)
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_parallel_assign_map_mismatch() {
    let src = r#"
package main

fn sink(m map[string]int32, x int) {}

fn main() {
    var m1 map[string]int32
    var m2 map[string]int64
    var a int
    var b int
    m1, a = m2, b
    sink(m1, a)
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_parallel_assign_pointer_mismatch() {
    let src = r#"
package main

fn sink(p *int32, x int) {}

fn main() {
    var p1 *int32
    var p2 *int64
    var a int
    var b int
    p1, a = p2, b
    sink(p1, a)
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_parallel_assign_slice_mismatch() {
    let src = r#"
package main

fn sink(s []int32, x int) {}

fn main() {
    var s1 []int32
    var s2 []int64
    var a int
    var b int
    s1, a = s2, b
    sink(s1, a)
}
"#;
    assert_reject_e1002(src);
}

#[test]
fn test_assign_chan_identical_accepts() {
    let src = r#"
package main

fn sink(c chan int32) {}

fn main() {
    var c1 chan int32
    var c2 chan int32
    c1 = c2
    sink(c1)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty(), "expected 0 diags, got: {:?}", r.diagnostics);
}

#[test]
fn test_assign_map_identical_accepts() {
    let src = r#"
package main

fn sink(m map[string]int32) {}

fn main() {
    var m1 map[string]int32
    var m2 map[string]int32
    m1 = m2
    sink(m1)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty(), "expected 0 diags, got: {:?}", r.diagnostics);
}

#[test]
fn test_assign_slice_identical_accepts() {
    let src = r#"
package main

fn sink(s []int32) {}

fn main() {
    var s1 []int32
    var s2 []int32
    s1 = s2
    sink(s1)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty(), "expected 0 diags, got: {:?}", r.diagnostics);
}

#[test]
fn test_assign_pointer_identical_accepts() {
    let src = r#"
package main

fn sink(p *int32) {}

fn main() {
    var p1 *int32
    var p2 *int32
    p1 = p2
    sink(p1)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty(), "expected 0 diags, got: {:?}", r.diagnostics);
}

#[test]
fn test_assign_array_alias_identical_accepts() {
    let src = r#"
package main

type arr1 = [3]int32

fn sink(a arr1) {}

fn main() {
    var a1 arr1
    var a2 arr1
    a1 = a2
    sink(a1)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty(), "expected 0 diags, got: {:?}", r.diagnostics);
}

#[test]
fn test_parallel_assign_valid_accepts() {
    let src = r#"
package main

fn sink(c chan int, m map[string]int) {}

fn main() {
    var c1 chan int
    var c2 chan int
    var m1 map[string]int
    var m2 map[string]int
    c1, m1 = c2, m2
    sink(c1, m1)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
    assert!(r.diagnostics.is_empty(), "expected 0 diags, got: {:?}", r.diagnostics);
}
