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
}
";

#[test]
fn legal_iterable_forms_stay_accepted() {
    // Precision control: every LX13 iterable kind keeps working — slice
    // binding, slice literal, string, map, channel. (The integer range
    // forms are a different statement kind and are pinned separately
    // below — they never enter the iterable classification.)
    let r = check(LEGAL_ITERABLES);
    assert!(
        codes(&r).is_empty(),
        "all LX13 iterable forms must stay clean, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn integer_range_forms_unaffected() {
    // `for i in 0..n` / `for i in 1..=8` are `ForKind::Range` — a
    // different statement kind with no iterable operand.
    let r = check(
        "fn main() {\n    for i in 0..3 {\n        fmt.println(i)\n    }\n    for i in 1..=8 {\n        fmt.println(i)\n    }\n}\n",
    );
    assert!(
        codes(&r).is_empty(),
        "range forms must stay clean, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

const BORROW_VIEW_ITERABLE: &str = "\
import \"rust/serde_json\"

fn main() {
    doc := serde_json.parse(`{\"users\":[]}`)?
    for u in doc.array(\"users\")? {
        _ = u
    }
}
";

#[test]
fn borrow_view_iterable_stays_accepted() {
    // The BorrowHandle whitelist arm — corpus-driven (example 05 /
    // corpus 07/11/12): the element VIEW the emitter iterates, not a Go
    // range shape of its own. Locks the arm so it is not removed.
    let r = check(BORROW_VIEW_ITERABLE);
    assert!(
        codes(&r).is_empty(),
        "the borrow-view iterable must stay clean, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

const USER_METHOD_CALL_ITERABLE: &str = "\
struct cache[k comparable] {
    items []k
}

fn (c *cache[k]) keys() []k {
    return c.items
}

fn main() {
    c := cache[int]{items: []int{1, 2}}
    for v in c.keys() {
        fmt.println(v)
    }
}
";

#[test]
fn user_method_call_iterable_stays_accepted() {
    // H2 pin: a user-method call result resolves through `method_decls`
    // (the same resolution `ty_of_expr` applies) — `call_shape` used to
    // leave it `Unknown`, fail-closing the iterable as E3005.
    let r = check(USER_METHOD_CALL_ITERABLE);
    assert!(
        codes(&r).is_empty(),
        "a user-method call returning a slice must stay clean, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

const FOREIGN_HANDLE_VALUE_ITERABLE: &str = "\
import \"rust/serde_json\"

fn main() {
    arr := serde_json.parse(`[]`)?
    for u in arr {
        _ = u
    }
}
";

#[test]
fn foreign_element_source_handle_iterable_stays_accepted() {
    // M1 pin: the emitter's ForeignTy range branch ranges the
    // `serde_json.value` handle as an element source; the checker must
    // not contradict that contract. Other handles (`flate2.writer`)
    // stay rejected — the whitelist is the narrow predicate, not the
    // whole OwnHandle family.
    let r = check(FOREIGN_HANDLE_VALUE_ITERABLE);
    assert!(
        codes(&r).is_empty(),
        "the element-source handle iterable must stay clean, got {:?}",
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

#[test]
fn set_range_iteration_accepts_single_var() {
    let r = check("fn main() {\n    seen := set[int]{}\n    for v in seen {\n    }\n}\n");
    assert!(
        codes(&r).is_empty(),
        "for v in set must be accepted, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn set_range_iteration_rejects_two_vars() {
    let r = check("fn main() {\n    seen := set[int]{}\n    for i, v in seen {\n    }\n}\n");
    assert_eq!(codes(&r), vec!["HYB-E2009"]);
    assert_eq!(r.verdict, Verdict::Reject);
}

