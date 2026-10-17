//! range_iter_target.rs — regression tests for issue #411: `for x in e`
//! iterables were never classified.
//!
//! SPEC-LANGUAGE LX13 pins the iterable set — "`for x in e` iterates
//! slices, arrays, strings (runes), maps (keys), channels (until close),
//! and integer ranges `for i in 0..n`" — but nothing checked that the `e`
//! of a `ForKind::Iterate` belongs to it. `for v in 5` / `for v in true`
//! passed `hyb check` with 0 diagnostics and emitted `for _, v := range x`
//! — invalid Go (`cannot range over 5`). This mirrors the send-target
//! (#77, HYB-E3007) and receive-operand (#409, HYB-E2008) twins:
//!   * a concrete non-iterable is HYB-E2009 (new registry code);
//!   * an unresolvable iterable stays fail-closed HYB-E3005 (O4.9).

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

const GENERIC_RECEIVER_FIELD_ITERABLE: &str = "\
struct box2[t any] {
    items []t
}

fn (b *box2[t]) walk() {
    for i, x in b.items {
    }
}

fn main() {
}
";

#[test]
fn generic_receiver_field_iterable_stays_accepted() {
    // Spike6-07 guard: a generic instantiation `box2[t]` folds its type
    // arguments into the name string (the parser contract gap noted in
    // parse_type), which used to leave the receiver's shape `Unknown` —
    // so `b.items` (the declared `[]t`) fail-closed as E3005. The shape
    // layer now resolves struct/alias table lookups through the base
    // name, and the declared slice field classifies as iterable.
    let r = check(GENERIC_RECEIVER_FIELD_ITERABLE);
    assert!(
        codes(&r).is_empty(),
        "a generic receiver's declared slice field must stay clean, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

const INT_BINDING_ITERABLE: &str = "\
fn main() {
    x := 5
    for v in x {
    }
}
";

#[test]
fn int_binding_iterable_is_rejected_e2009() {
    // Repro A from the issue: a plain int binding. Concrete non-iterable
    // shape -> HYB-E2009 (registry: non-iterable-range).
    let r = check(INT_BINDING_ITERABLE);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2009"),
        "expected HYB-E2009 for the int iterable, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const BOOL_AND_INT_EXPR_ITERABLE: &str = "\
fn main() {
    b := true
    for v in b {
    }
    for v in 1 + 2 {
    }
}
";

#[test]
fn bool_and_int_expression_iterables_are_rejected() {
    // Repro B/C: a bool binding and an int-typed expression — both
    // concrete non-iterables, one diagnostic each.
    let r = check(BOOL_AND_INT_EXPR_ITERABLE);
    let cs = codes(&r);
    assert_eq!(
        cs.iter().filter(|c| c.as_str() == "HYB-E2009").count(),
        2,
        "expected exactly two HYB-E2009 for bool + int expr, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const UNRESOLVABLE_ITERABLE: &str = "\
fn noop() {
}

fn main() {
    for v in noop() {
    }
}
";

#[test]
fn unresolvable_iterable_is_fail_closed_e3005() {
    // The void-call twin: the iterable's shape does not resolve (no
    // results -> Unknown), so the fail-closed O4.9 path fires
    // (HYB-E3005) — never OK again, same as the send/receive sides.
    let r = check(UNRESOLVABLE_ITERABLE);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3005"),
        "expected HYB-E3005 for the void-call iterable, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const CLOSURE_BODY_NON_ITERABLE: &str = "\
fn main() {
    x := 5
    body := fn() {
        for v in x {
        }
    }
    body()
}
";

#[test]
fn closure_body_non_iterable_is_rejected() {
    // The closure-body twin (closures.rs check path — FnLit bodies sit
    // outside the Tier-0 flat walk, the A5 perimeter).
    let r = check(CLOSURE_BODY_NON_ITERABLE);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2009"),
        "expected HYB-E2009 in the closure body, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const NESTED_BLOCK_NON_ITERABLE: &str = "\
fn main() {
    x := 5
    if true {
        for v in x {
        }
    }
}
";

#[test]
fn nested_block_non_iterable_is_rejected() {
    // A defer-style twin for nesting: the if-body statement arrives as
    // its own flattened entry (or via the closure walk) and must not be
    // skipped.
    let r = check(NESTED_BLOCK_NON_ITERABLE);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E2009"),
        "expected HYB-E2009 inside the if body, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const LEGAL_ITERABLES: &str = "\
fn main() {
    xs := []int{1, 2, 3}
    for v in xs {
    }
    for _, p in [\"a\", \"b\"] {
    }
    s := \"abc\"
    for c in s {
    }
    m := map[string]int{\"k\": 1}
    for k in m {
    }
    ch := make(chan int, 1)
    for v in ch {
    }
    for i in 0..3 {
    }
    for i in 1..=8 {
    }
}
";

#[test]
fn legal_iterable_forms_stay_accepted() {
    // Precision control: every LX13 iterable kind keeps working — slice
    // binding, slice literal, string, map, channel, and both integer
    // range forms (Range is a different statement kind, unaffected).
    let r = check(LEGAL_ITERABLES);
    assert!(
        codes(&r).is_empty(),
        "all LX13 iterable forms must stay clean, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

const CLOSURE_BODY_LEGAL_ITERABLE: &str = "\
fn main() {
    body := fn() {
        for c in \"abc\" {
        }
    }
    body()
}
";

#[test]
fn closure_body_legal_iterable_stays_accepted() {
    // The accepted twin of the closure rejection above — the closure
    // walk classifies but does not over-reject. (The closure iterates a
    // string literal directly; a direct call, not a spawn, keeps the
    // sendability rules out of the picture.)
    let r = check(CLOSURE_BODY_LEGAL_ITERABLE);
    assert!(
        codes(&r).is_empty(),
        "a closure iterating a slice must stay clean, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}
