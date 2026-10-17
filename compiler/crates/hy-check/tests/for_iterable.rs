//! for_iterable.rs — regression tests for issue #411:
//! `for x in e` never type-checked the iterable, allowing ranging over an
//! int, bool, struct, or non-iterable call result to pass `hyb check` with
//! 0 diagnostics and emit non-compiling Go (`cannot range over`).
//!
//! SPEC-LANGUAGE LX13 defines iterable expressions:
//!   slices, arrays, strings (runes), maps (keys), channels, and borrow handles.
//!
//! Diagnostic contract:
//!   - Concrete non-iterables reject as HYB-E2009 (rule LX13).
//!   - Unresolvable shapes fail-closed as HYB-E3005 (rule O4.9).
//!   - Slices, arrays, strings, maps, channels, and integer ranges keep passing.

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
// Negative Cases: Non-iterables are rejected
// ---------------------------------------------------------------------------

const R1_LOCAL_INT: &str = "\
fn main() {
    x := 5
    for v in x {
    }
}
";

#[test]
fn r1_local_int_iterable_rejected_e2008() {
    let r = check(R1_LOCAL_INT);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2009"),
        "expected HYB-E2009 for local int iterable, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const R2_CALL_INT: &str = "\
fn f() int {
    return 5
}

fn main() {
    for v in f() {
    }
}
";

#[test]
fn r2_call_int_iterable_rejected_e2008() {
    let r = check(R2_CALL_INT);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2009"),
        "expected HYB-E2009 for call int iterable, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const R3_LOCAL_BOOL: &str = "\
fn main() {
    b := true
    for v in b {
    }
}
";

#[test]
fn r3_local_bool_iterable_rejected_e2008() {
    let r = check(R3_LOCAL_BOOL);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2009"),
        "expected HYB-E2009 for bool iterable, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const R4_STRUCT_ITERABLE: &str = "\
struct container {
    val int
}

fn main() {
    c := container{val: 10}
    for v in c {
    }
}
";

#[test]
fn r4_struct_iterable_rejected_e2008() {
    let r = check(R4_STRUCT_ITERABLE);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2009"),
        "expected HYB-E2009 for struct iterable, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const R5_TWO_VAR_NON_ITERABLE: &str = "\
fn main() {
    x := 5
    for i, v in x {
    }
}
";

#[test]
fn r5_two_var_non_iterable_rejected_e2008() {
    let r = check(R5_TWO_VAR_NON_ITERABLE);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2009"),
        "expected HYB-E2009 for two-variable non-iterable, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const R6_CLOSURE_NON_ITERABLE: &str = "\
fn main() {
    x := 5
    body := fn() {
        for v in x {
        }
    }
    task body()
}
";

#[test]
fn r6_closure_non_iterable_rejected_e2008() {
    let r = check(R6_CLOSURE_NON_ITERABLE);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2009"),
        "expected HYB-E2009 in closure body iterable, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const R7_VOID_CALL_ITERABLE: &str = "\
fn noop() {
}

fn main() {
    for v in noop() {
    }
}
";

#[test]
fn r7_void_call_iterable_rejected_e3005() {
    let r = check(R7_VOID_CALL_ITERABLE);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3005" || c == "HYB-E2009"),
        "expected HYB-E3005/E2008 for unresolvable void call iterable, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

// ---------------------------------------------------------------------------
// Positive Cases: Valid iterables stay cleanly accepted
// ---------------------------------------------------------------------------

const P1_INTEGER_RANGE: &str = "\
fn main() {
    for i in 0..10 {
    }
}
";

#[test]
fn p1_integer_range_stays_accepted() {
    let r = check(P1_INTEGER_RANGE);
    assert!(
        codes(&r).is_empty(),
        "integer range must stay accepted, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

const P2_SLICE_ITERABLE: &str = "\
fn main() {
    s := []int{1, 2, 3}
    for v in s {
    }
}
";

#[test]
fn p2_slice_iterable_stays_accepted() {
    let r = check(P2_SLICE_ITERABLE);
    assert!(
        codes(&r).is_empty(),
        "slice iterable must stay accepted, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

const P3_MAP_ITERABLE: &str = "\
fn main() {
    m := map[string]int{\"k\": 1}
    for k, v in m {
    }
}
";

#[test]
fn p3_map_iterable_stays_accepted() {
    let r = check(P3_MAP_ITERABLE);
    assert!(
        codes(&r).is_empty(),
        "map iterable must stay accepted, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

const P4_STRING_ITERABLE: &str = "\
fn main() {
    str := \"hello\"
    for c in str {
    }
}
";

#[test]
fn p4_string_iterable_stays_accepted() {
    let r = check(P4_STRING_ITERABLE);
    assert!(
        codes(&r).is_empty(),
        "string iterable must stay accepted, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

const P5_CHAN_ITERABLE: &str = "\
fn main() {
    ch := make(chan int, 1)
    for v in ch {
    }
}
";

#[test]
fn p5_chan_iterable_stays_accepted() {
    let r = check(P5_CHAN_ITERABLE);
    assert!(
        codes(&r).is_empty(),
        "channel iterable must stay accepted, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}
