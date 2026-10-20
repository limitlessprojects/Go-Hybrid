//! untyped_nil_decl.rs — N1's untyped leg (HYB-E4003): `var x = nil`,
//! `x := nil`, `let x = nil` — a declaration with NO ascription. The E4001
//! sites all require a declared type to prove the nil against; with no
//! ascription `shape_of_expr(nil)` is Unknown, the checker used to accept
//! the declaration, and the emitter emitted `var x = nil` — which Go
//! rejects outright (`use of untyped nil in variable declaration`).
//! Accepted-but-unbuildable (BUILDABLE rule). Local `const c = nil` is
//! the E6003 constant-expression guard (issue #640) — different locus.

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
// The four spellings that Go rejects: var x = nil / x := nil / let x = nil /
// parallel untyped nil in a multi-value define.
// ---------------------------------------------------------------------------

#[test]
fn var_untyped_nil_is_e4003() {
    let r = check("fn main() {\n    var ch = nil\n    _ = ch\n}\n");
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E4003"),
        "expected HYB-E4003 for `var ch = nil`, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn short_define_untyped_nil_is_e4003() {
    let r = check("fn main() {\n    ch := nil\n    _ = ch\n}\n");
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E4003"),
        "expected HYB-E4003 for `ch := nil`, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn let_untyped_nil_is_e4003() {
    let r = check("fn main() {\n    let x = nil\n    _ = x\n}\n");
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E4003"),
        "expected HYB-E4003 for `let x = nil`, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn parallel_untyped_nil_is_e4003() {
    // `a, b := nil, 1` — the nil half is an untyped declaration too.
    let r = check("fn main() {\n    a, b := nil, 1\n    _ = a\n    _ = b\n}\n");
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E4003"),
        "expected HYB-E4003 for the nil half of `a, b := nil, 1`, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

// ---------------------------------------------------------------------------
// Every body shape: closure, loop, match arm, if-block — the flat walk and
// the closure boundary walk both carry the check.
// ---------------------------------------------------------------------------

#[test]
fn closure_body_untyped_nil_is_e4003() {
    let r = check(
        "fn main() {\n    g := fn() {\n        var x = nil\n        _ = x\n    }\n    g()\n}\n",
    );
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E4003"),
        "expected HYB-E4003 for a closure-body `var x = nil`, got {:?}",
        cs
    );
}

#[test]
fn loop_body_untyped_nil_is_e4003() {
    let r = check("fn main() {\n    for i in 0..3 {\n        var x = nil\n        _ = x\n    }\n}\n");
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E4003"),
        "expected HYB-E4003 for a loop-body `var x = nil`, got {:?}",
        cs
    );
}

#[test]
fn match_arm_untyped_nil_is_e4003() {
    let r = check(
        "fn main() {\n    match 1 {\n        1 => {\n            var y = nil\n            _ = y\n        }\n        _ => {}\n    }\n}\n",
    );
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E4003"),
        "expected HYB-E4003 for a match-arm `var y = nil`, got {:?}",
        cs
    );
}

#[test]
fn if_body_untyped_nil_is_e4003() {
    // Carried from dezzy-music's parallel take #691 (same fix, credit both):
    // an if-body `var x = nil` rides the same closure-boundary walk.
    let r = check("fn main() {\n    if true {\n        var x = nil\n        _ = x\n    }\n}\n");
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E4003"),
        "expected HYB-E4003 for an if-body `var x = nil`, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn if_init_untyped_nil_is_e4003() {
    // Carried from dezzy-music's parallel take #691 (same fix, credit both):
    // the if-statement initializer `if x := nil; ...` is a local declaration.
    let r = check("fn main() {\n    if x := nil; x != nil {\n        _ = x\n    }\n}\n");
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E4003"),
        "expected HYB-E4003 for the if-init `x := nil`, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

// ---------------------------------------------------------------------------
// Locals named `_` still reject: the emitted Go `var _ = nil` is just as
// unbuildable.
// ---------------------------------------------------------------------------

#[test]
fn blank_untyped_nil_is_e4003() {
    let r = check("fn main() {\n    var _ = nil\n}\n");
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E4003"),
        "expected HYB-E4003 for `var _ = nil`, got {:?}",
        cs
    );
}

// ---------------------------------------------------------------------------
// Controls that must stay ACCEPTED — the nil HAS a declared home, or is not
// a declaration at all.
// ---------------------------------------------------------------------------

#[test]
fn ascribed_error_nil_stays_legal() {
    // N1 home #1, ascribed: `var e error = nil` is valid Go and Hybrid.
    let r = check("fn main() {\n    var e error = nil\n    _ = e\n}\n");
    assert!(
        codes(&r).is_empty(),
        "ascribed error-nil must stay legal, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn bare_nil_arg_stays_legal() {
    // `f(nil)` — the call site gives nil a type context; not a declaration.
    let r = check("fn sink(m map[string]int) int { return m[\"k\"] }\n\nfn main() {\n    x := sink(nil)\n    _ = x\n}\n");
    assert!(
        codes(&r).iter().all(|c| c != "HYB-E4003"),
        "bare nil as a call argument is not a declaration; got {:?}",
        codes(&r)
    );
}

#[test]
fn bare_nil_compare_stays_legal() {
    // `ch == nil` — a comparison, not a declaration.
    let r = check("fn main() {\n    ch := make(chan int, 1)\n    if ch == nil {\n        return\n    }\n}\n");
    assert!(
        codes(&r).iter().all(|c| c != "HYB-E4003"),
        "bare nil in a comparison is not a declaration; got {:?}",
        codes(&r)
    );
}

#[test]
fn zero_value_decl_stays_legal() {
    // The N3 fix-it route: declare without an initializer.
    let r = check("fn main() {\n    var ch chan int\n    var s []int\n    var m map[string]int\n    _ = ch\n    _ = s\n    _ = m\n}\n");
    assert!(
        codes(&r).is_empty(),
        "zero-value declarations must stay legal, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn local_const_nil_is_not_e4003() {
    // Local `const c = nil` is the E6003 constant-expression guard's locus
    // (issue #640) — E4003 must not double-fire on it. (On main today the
    // E6003 guard accepts nil literals; the fix lands via issue #640.)
    let r = check("fn main() {\n    const c = nil\n    _ = c\n}\n");
    let cs = codes(&r);
    assert!(
        cs.iter().all(|c| c != "HYB-E4003"),
        "local `const c = nil` belongs to the E6003 guard (#640), not E4003; got {:?}",
        cs
    );
}
