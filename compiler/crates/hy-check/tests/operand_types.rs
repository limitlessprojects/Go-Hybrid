//! operand_types.rs — regression tests for issue #425:
//! Operand types were never checked for index, slice, call, and binding-shaped
//! builtin misuse (len, cap, close, delete, append), allowing accepted programs
//! to emit unbuildable Go.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

// ---------------------------------------------------------------------------
// Negative Cases: C. Index / slice of non-indexable value (HYB-E2009)
// ---------------------------------------------------------------------------

const C1_INDEX_INT: &str = "\
fn main() {
    x := 5
    v := x[0]
}
";

#[test]
fn c1_index_int_rejected_e2009() {
    let r = check(C1_INDEX_INT);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2009"),
        "expected HYB-E2009 for index of int, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const C2_SLICE_INT: &str = "\
fn main() {
    x := 5
    v := x[0:1]
}
";

#[test]
fn c2_slice_int_rejected_e2009() {
    let r = check(C2_SLICE_INT);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2009"),
        "expected HYB-E2009 for slice of int, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

// ---------------------------------------------------------------------------
// Negative Cases: D. Calling a non-function (HYB-E2009)
// ---------------------------------------------------------------------------

const D1_CALL_INT: &str = "\
fn main() {
    x := 5
    x()
}
";

#[test]
fn d1_call_int_rejected_e2009() {
    let r = check(D1_CALL_INT);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2009"),
        "expected HYB-E2009 for call of int, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

// ---------------------------------------------------------------------------
// Negative Cases: E. Builtin operand kind via a binding (HYB-E2005)
// ---------------------------------------------------------------------------

const E1_LEN_INT: &str = "\
fn main() {
    x := 5
    l := len(x)
}
";

#[test]
fn e1_len_int_rejected_e2005() {
    let r = check(E1_LEN_INT);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for len(int), got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const E2_CLOSE_NON_CHAN: &str = "\
fn main() {
    x := 5
    close(x)
}
";

#[test]
fn e2_close_non_chan_rejected_e2005() {
    let r = check(E2_CLOSE_NON_CHAN);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for close(int), got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const E3_CAP_MAP: &str = "\
fn main() {
    m := map[string]int{}
    c := cap(m)
}
";

#[test]
fn e3_cap_map_rejected_e2005() {
    let r = check(E3_CAP_MAP);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for cap(map), got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const E4_DELETE_NON_MAP: &str = "\
fn main() {
    xs := []int{1}
    delete(xs, 0)
}
";

#[test]
fn e4_delete_non_map_rejected_e2005() {
    let r = check(E4_DELETE_NON_MAP);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for delete(slice, 0), got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const E5_APPEND_NON_SLICE: &str = "\
fn main() {
    x := 5
    x = append(x, 1)
}
";

#[test]
fn e5_append_non_slice_rejected_e2005() {
    let r = check(E5_APPEND_NON_SLICE);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2005"),
        "expected HYB-E2005 for append(int, 1), got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

// ---------------------------------------------------------------------------
// Negative Cases: A. Non-boolean condition (HYB-E2009)
// ---------------------------------------------------------------------------

const A1_NON_BOOL_IF_STRING: &str = "\
fn main() {
    s := \"hello\"
    if s {
        fmt.println(1)
    }
}
";

#[test]
fn a1_non_bool_if_string_rejected_e2009() {
    let r = check(A1_NON_BOOL_IF_STRING);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2009"),
        "expected HYB-E2009 for non-bool if string condition, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

// ---------------------------------------------------------------------------
// Positive Cases: Valid operations stay accepted
// ---------------------------------------------------------------------------

const P1_VALID_OPERATIONS: &str = "\
fn main() {
    xs := []int{1, 2, 3}
    v := xs[0]
    sub := xs[0:2]
    xs = append(xs, 4)
    l := len(xs)
    c := cap(xs)

    m := map[string]int{\"k\": 1}
    mv := m[\"k\"]
    delete(m, \"k\")

    ch := make(chan int, 1)
    close(ch)
}
";

#[test]
fn p1_valid_operations_stay_accepted() {
    let r = check(P1_VALID_OPERATIONS);
    assert!(
        codes(&r).is_empty(),
        "valid operations must stay accepted, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}
