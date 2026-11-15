//! Issue #3518: Builtin delete and append accept untyped nil for non-nilable key and element types (Row 13 BUILDABLE).
//!
//! Go refuses untyped nil passed to non-nilable keys in `delete` and non-nilable elements in `append`:
//! - delete(map[int]V, nil) -> cannot use nil as int value in argument to delete
//! - delete(map[string]V, nil) -> cannot use nil as string value in argument to delete
//! - delete(set[int], nil) -> cannot use nil as int value
//! - append([]int, nil) -> cannot use nil as int value in argument to append
//! - append([]string, nil) -> cannot use nil as string value in argument to append
//!
//! Nilable key and element types remain accepted:
//! - pointers *T
//! - interfaces (any)
//! - slices []T (for append)
//! - maps map[K]V (for append)
//! - channels chan T (for delete and append)
//! - functions fn(...) (for append)

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "test.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn errors(r: &hy_check::CheckResult) -> Vec<(String, String)> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| (d.code.clone(), d.message.clone()))
        .collect()
}

fn assert_rejected(src: &str, expected_code: &str, what: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected Reject for {what}, got Accept (0 diagnostics)"
    );
    let errs = errors(&r);
    assert!(
        errs.iter().any(|(code, _)| code == expected_code),
        "expected {expected_code} for {what}, got {errs:?}"
    );
}

#[test]
fn rejects_nil_key_for_non_nilable_map_keys() {
    assert_rejected(
        "package main\nfn main() {\n    var m map[int32]string\n    delete(m, nil)\n}",
        "HYB-E2005",
        "delete map[int32] with nil key",
    );
    assert_rejected(
        "package main\nfn main() {\n    var m map[string]int32\n    delete(m, nil)\n}",
        "HYB-E2005",
        "delete map[string] with nil key",
    );
    assert_rejected(
        "package main\nfn main() {\n    var m map[bool]int32\n    delete(m, nil)\n}",
        "HYB-E2005",
        "delete map[bool] with nil key",
    );
    assert_rejected(
        "package main\nstruct point { x int32 }\nfn main() {\n    var m map[point]int32\n    delete(m, nil)\n}",
        "HYB-E2005",
        "delete map[struct] with nil key",
    );
    assert_rejected(
        "package main\nfn main() {\n    var m map[[3]int32]int32\n    delete(m, nil)\n}",
        "HYB-E2005",
        "delete map[array] with nil key",
    );
}

#[test]
fn rejects_nil_element_for_non_nilable_set() {
    assert_rejected(
        "package main\nfn main() {\n    s := set[int32]{}\n    delete(s, nil)\n}",
        "HYB-E2005",
        "delete set[int32] with nil element",
    );
}

#[test]
fn rejects_nil_element_for_non_nilable_slices_in_append() {
    assert_rejected(
        "package main\nfn main() {\n    var is []int32\n    _ = append(is, nil)\n}",
        "HYB-E1002",
        "append []int32 with nil",
    );
    assert_rejected(
        "package main\nfn main() {\n    var ss []string\n    _ = append(ss, nil)\n}",
        "HYB-E1002",
        "append []string with nil",
    );
    assert_rejected(
        "package main\nfn main() {\n    var bs []bool\n    _ = append(bs, nil)\n}",
        "HYB-E1002",
        "append []bool with nil",
    );
    assert_rejected(
        "package main\nstruct point { x int32 }\nfn main() {\n    var pts []point\n    _ = append(pts, nil)\n}",
        "HYB-E1002",
        "append []struct with nil",
    );
    assert_rejected(
        "package main\nfn main() {\n    var arrs [][3]int32\n    _ = append(arrs, nil)\n}",
        "HYB-E1002",
        "append [][3]int32 with nil",
    );
}

#[test]
fn accepts_nil_for_nilable_keys_and_elements() {
    let src = r#"
package main

struct node { val int32 }

fn main() {
    // Nilable map keys in delete
    var m_ptr map[*node]int32
    delete(m_ptr, nil)

    var m_iface map[any]int32
    delete(m_iface, nil)

    var m_chan map[chan int32]int32
    delete(m_chan, nil)

    // Nilable slice elements in append
    var ps []*node
    _ = append(ps, nil)

    var anys []any
    _ = append(anys, nil)

    var slices [][]int32
    _ = append(slices, nil)

    var maps []map[string]int32
    _ = append(maps, nil)

    var chans []chan int32
    _ = append(chans, nil)

    var fns []fn(int32)
    _ = append(fns, nil)
}
"#;
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected Accept for nilable keys and elements, got {:?}",
        r.diagnostics
    );
}
