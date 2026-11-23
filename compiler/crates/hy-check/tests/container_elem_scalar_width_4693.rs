//! container_elem_scalar_width_4693.rs — regression for issue #4693:
//! container element scalar width mismatches (`[]int64` -> `[]int32`,
//! `map[K]int64` -> `map[K]int32`, `*int64` -> `*int32`) bypassed the
//! assignment and channel-send checks because only the outer kind was
//! compared. `hyb check` passed programs `go build` rejects.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn has_e1002(r: &hy_check::CheckResult) -> bool {
    r.diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error && d.code == "HYB-E1002")
}

#[test]
fn slice_elem_width_assign_rejects() {
    let r = check(
        "fn main() {\n    var s32 []int32\n    var s64 []int64\n    s32 = s64\n    _ = s32\n}\n",
    );
    assert!(has_e1002(&r), "expected E1002 for []int64 -> []int32, got {:?}", r.diagnostics.iter().map(|d| d.code.clone()).collect::<Vec<_>>());
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn map_value_width_assign_rejects() {
    let r = check(
        "fn main() {\n    var m32 map[string]int32\n    var m64 map[string]int64\n    m32 = m64\n    _ = m32\n}\n",
    );
    assert!(has_e1002(&r));
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn pointer_pointee_width_assign_rejects() {
    let r = check(
        "fn main() {\n    var p32 *int32\n    var p64 *int64\n    p32 = p64\n    _ = p32\n}\n",
    );
    assert!(has_e1002(&r));
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn parallel_assign_elem_width_rejects() {
    let r = check(
        "fn main() {\n    var s1 []int32\n    var s2 []int64\n    var m1 map[string]int32\n    var m2 map[string]int64\n    s1, m1 = s2, m2\n    _ = s1\n    _ = m1\n}\n",
    );
    assert!(has_e1002(&r));
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn chan_send_container_elem_width_rejects() {
    // chan of a numeric element is sendable; use int32/int64 element to
    // isolate the width rule from the O4 sendability rule.
    let r = check(
        "fn main() {\n    var ch chan int32\n    var v int64 = 1\n    ch <- v\n}\n",
    );
    assert!(has_e1002(&r), "expected E1002 for int64 -> chan int32, got {:?}", r.diagnostics.iter().map(|d| d.code.clone()).collect::<Vec<_>>());
}

#[test]
fn same_width_assigns_accept() {
    let r = check(
        "fn main() {\n    var s32 []int32\n    var s32b []int32\n    s32 = s32b\n    var m32 map[string]int32\n    var m32b map[string]int32\n    m32 = m32b\n    var p32 *int32\n    var p32b *int32\n    p32 = p32b\n    var s1 []int32\n    var m1 map[string]int32\n    s1, m1 = s32b, m32b\n    _ = s32\n    _ = m32\n    _ = p32\n    _ = s1\n    _ = m1\n}\n",
    );
    assert!(
        !has_e1002(&r),
        "expected same-width assigns to accept, got {:?}",
        r.diagnostics.iter().map(|d| (d.code.clone(), d.message.clone())).collect::<Vec<_>>()
    );
}

#[test]
fn matching_width_chan_send_accepts_kind() {
    // Untyped constant into a chan int32 stays legal (constant assignability).
    let r = check(
        "fn main() {\n    var ch chan int32\n    ch <- 5\n}\n",
    );
    assert!(!has_e1002(&r), "got {:?}", r.diagnostics.iter().map(|d| d.code.clone()).collect::<Vec<_>>());
}
