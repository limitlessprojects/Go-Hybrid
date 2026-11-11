//! Regression tests for issue #974:
//!
//! 1. `callee_shared_return_pairs` (`HYB-E3003`): conditional branch rebinding
//!    and inner-block shadowing must join reachable branch states and restore
//!    shadowed outer bindings on scope exit instead of clobbering return alias
//!    origins linearly.
//! 2. `walk_callee_const_return_roots` (`HYB-E3006` / `HYB-E3004`): conditional
//!    branches, inner-block shadowing, loops (`for .. in`), `match`, `select`,
//!    and top-level callee isolation from caller locals must preserve
//!    file-level `const` return origins.
//! 3. Caller-side shadowed `const` returns: when a caller local shadows a
//!    file-level `const` name (`shared := 0`), receiving or matching against
//!    a callee return of `const shared` still propagates `const` immutability.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let parsed = parse_file("<test>", src).expect("unexpected parse errors");
    check_file(&parsed)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn conditional_rebind_one_branch_keeps_shared_return_pair_e3003() {
    let src = "\
fn pair(flag bool) (map[string]int, map[string]int) {
    m := map[string]int{\"a\": 1}
    m2 := m
    if flag {
        m2 = map[string]int{\"b\": 2}
    }
    return m, m2
}

fn worker(x map[string]int, done chan<- bool) {
    x[\"k\"] = 1
    done <- true
}

fn main() {
    done := make(chan bool, 1)
    a, b := pair(true)
    task worker(move a, done)
    b[\"k\"] = 2
    <-done
}
";
    let res = check(src);
    let codes = error_codes(&res);
    assert_eq!(res.verdict, Verdict::Reject, "expected Reject, got {codes:?}");
    assert!(
        codes.contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003 when only one branch rebinds `m2`, got: {codes:?}"
    );
}

#[test]
fn inner_block_shadow_does_not_clobber_shared_return_pair_e3003() {
    let src = "\
fn pair(flag bool) (map[string]int, map[string]int) {
    m := map[string]int{\"a\": 1}
    m2 := m
    if flag {
        m := 0
        _ = m
    }
    return m, m2
}

fn worker(x map[string]int, done chan<- bool) {
    x[\"k\"] = 1
    done <- true
}

fn main() {
    done := make(chan bool, 1)
    a, b := pair(true)
    task worker(move a, done)
    b[\"k\"] = 2
    <-done
}
";
    let res = check(src);
    let codes = error_codes(&res);
    assert_eq!(res.verdict, Verdict::Reject, "expected Reject, got {codes:?}");
    assert!(
        codes.contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003 across inner-block scalar shadow of `m`, got: {codes:?}"
    );
}

#[test]
fn caller_shadowed_const_does_not_hide_callee_pair_or_const_writes() {
    let src = "\
const shared = map[string]int{\"k\": 1}

fn pair_const() (map[string]int, map[string]int) {
    return shared, shared
}

fn worker(x map[string]int, done chan<- bool) {
    _ = x
    done <- true
}

fn main() {
    shared := 0
    _ = shared
    done := make(chan bool, 1)
    a, b := pair_const()
    task worker(move a, done)
    b[\"x\"] = 2
    <-done
}
";
    let res = check(src);
    let codes = error_codes(&res);
    assert_eq!(res.verdict, Verdict::Reject, "expected Reject, got {codes:?}");
    assert!(
        codes.contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003 when caller shadows `const shared`, got: {codes:?}"
    );
    assert!(
        codes.contains(&"HYB-E3004".to_string()),
        "expected HYB-E3004 when moving returned `const shared`, got: {codes:?}"
    );
    assert!(
        codes.contains(&"HYB-E3006".to_string()),
        "expected HYB-E3006 when mutating second result of `pair_const()`, got: {codes:?}"
    );
}

#[test]
fn const_return_survives_inner_block_shadow_and_conditional_rebind() {
    let src_shadow = "\
const shared = map[string]int{\"k\": 1}

fn get_const(flag bool) map[string]int {
    if flag {
        shared := 0
        _ = shared
    }
    return shared
}

fn main() {
    shared := 0
    _ = shared
    m := get_const(true)
    m[\"x\"] = 1
}
";
    let r1 = check(src_shadow);
    let c1 = error_codes(&r1);
    assert_eq!(r1.verdict, Verdict::Reject, "expected Reject, got {c1:?}");
    assert!(
        c1.contains(&"HYB-E3006".to_string()),
        "expected HYB-E3006 after inner-block shadow inside callee, got: {c1:?}"
    );

    let src_inner_shadow = "\
const shared = map[string]int{\"k\": 1}

fn get_const_inner_shadow(flag bool) map[string]int {
    m := shared
    if flag {
        m := map[string]int{\"fresh\": 1}
        _ = m
    }
    return m
}

fn worker(x map[string]int, done chan<- bool) {
    _ = x
    done <- true
}

fn main() {
    done := make(chan bool, 1)
    m := get_const_inner_shadow(true)
    task worker(move m, done)
    <-done
}
";
    let r2 = check(src_inner_shadow);
    let c2 = error_codes(&r2);
    assert_eq!(r2.verdict, Verdict::Reject, "expected Reject, got {c2:?}");
    assert!(
        c2.contains(&"HYB-E3004".to_string()),
        "expected HYB-E3004 when inner branch shadows `m` without modifying outer `m`, got: {c2:?}"
    );
    assert!(
        !c2.contains(&"HYB-E3006".to_string()),
        "did not expect HYB-E3006 inside get_const_inner_shadow, got: {c2:?}"
    );
}

#[test]
fn for_iterate_and_match_propagate_const_and_shared_return_origins() {
    let src_for_const = "\
const shared = map[string]int{\"k\": 1}

fn from_for_in(flag bool) map[string]int {
    list := []map[string]int{shared}
    for _, item in list {
        if flag {
            return item
        }
    }
    return map[string]int{}
}

fn main() {
    m := from_for_in(true)
    m[\"x\"] = 2
}
";
    let r1 = check(src_for_const);
    let c1 = error_codes(&r1);
    assert_eq!(r1.verdict, Verdict::Reject, "expected Reject, got {c1:?}");
    assert!(
        c1.contains(&"HYB-E3006".to_string()),
        "expected HYB-E3006 via for-in return of const element, got: {c1:?}"
    );

    let src_match_const = "\
const shared = map[string]int{\"k\": 1}

fn get_opt_shared() option[map[string]int] {
    var opt option[map[string]int] = some(shared)
    match opt {
        some(v): { return some(v) }
        none: { return none }
    }
}

fn main() {
    shared := 0
    _ = shared
    match get_opt_shared() {
        some(v): { v[\"x\"] = 1 }
        none: {}
    }
}
";
    let r2 = check(src_match_const);
    let c2 = error_codes(&r2);
    assert_eq!(r2.verdict, Verdict::Reject, "expected Reject, got {c2:?}");
    assert!(
        c2.contains(&"HYB-E3006".to_string()),
        "expected HYB-E3006 through match + caller shadow, got: {c2:?}"
    );
}

#[test]
fn positive_controls_accept_fresh_all_branch_rebinds_and_shadowed_local() {
    let src_fresh_pair = "\
fn pair(flag bool) (map[string]int, map[string]int) {
    m := map[string]int{\"a\": 1}
    m2 := m
    if flag {
        m2 = map[string]int{\"b\": 2}
    } else {
        m2 = map[string]int{\"c\": 3}
    }
    return m, m2
}

fn worker(x map[string]int, done chan<- bool) {
    x[\"k\"] = 1
    done <- true
}

fn main() {
    done := make(chan bool, 1)
    a, b := pair(true)
    task worker(move a, done)
    b[\"k\"] = 2
    <-done
}
";
    let r1 = check(src_fresh_pair);
    assert_eq!(
        r1.verdict,
        Verdict::Accept,
        "all-branch fresh rebind should accept, got: {:?}",
        r1.diagnostics
    );

    let src_fresh_const_rebind = "\
const shared = map[string]int{\"k\": 1}

fn get_fresh(flag bool) map[string]int {
    m := map[string]int{\"a\": 1}
    if flag {
        m := shared
        _ = m
    }
    return m
}

fn main() {
    shared := map[string]int{\"local\": 1}
    shared[\"ok\"] = 2
    res := get_fresh(true)
    res[\"ok\"] = 3
}
";
    let r2 = check(src_fresh_const_rebind);
    assert_eq!(
        r2.verdict,
        Verdict::Accept,
        "inner-block shadow of outer fresh local + local shadowed map write should accept, got: {:?}",
        r2.diagnostics
    );

    let src_fresh_loop_pair = "\
fn pair_loop() (map[string]int, map[string]int) {
    a := map[string]int{\"a\": 1}
    b := map[string]int{\"b\": 2}
    for i in 0..3 {
        _ = i
        a = map[string]int{\"x\": 1}
        b = map[string]int{\"y\": 2}
    }
    return a, b
}

fn worker(x map[string]int, done chan<- bool) {
    x[\"k\"] = 1
    done <- true
}

fn main() {
    done := make(chan bool, 1)
    a, b := pair_loop()
    task worker(move a, done)
    b[\"k\"] = 2
    <-done
}
";
    let r3 = check(src_fresh_loop_pair);
    assert_eq!(
        r3.verdict,
        Verdict::Accept,
        "independent fresh loop rebinds should accept, got: {:?}",
        r3.diagnostics
    );
}
