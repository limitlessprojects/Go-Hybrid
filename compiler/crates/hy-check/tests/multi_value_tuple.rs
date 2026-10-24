//! multi_value_tuple.rs — regression tests for T23 (HYB-E2002): a
//! multi-value result is not a tuple. SPEC-LANGUAGE.md §2.8 pins
//! `p := minmax(3, 9)` as a compile-fail claim; before this check the
//! program was accepted with 0 diagnostics and `go build` failed with
//! `assignment mismatch: 1 variable but minmax returns 2 values`.

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

const MINMAX: &str =
    "fn minmax(a int, b int) (int, int) {\n    if a < b { return a, b }\n    return b, a\n}\n";

fn with_main(body: &str) -> String {
    format!("{MINMAX}\nfn main() {{\n{body}}}\n")
}

#[test]
fn spec_t23_single_define_is_rejected_e2002() {
    // The exact SPEC §2.8 claim.
    let r = check(&with_main("    p := minmax(3, 9)\n    fmt.println(p)\n"));
    assert_eq!(codes(&r), vec!["HYB-E2002".to_string()]);
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn let_and_var_single_bind_are_rejected_e2002() {
    let r = check(&with_main("    let p = minmax(3, 9)\n    fmt.println(p)\n"));
    assert_eq!(codes(&r), vec!["HYB-E2002".to_string()]);
    let r = check(&with_main(
        "    var p int = minmax(3, 9)\n    fmt.println(p)\n",
    ));
    assert_eq!(codes(&r), vec!["HYB-E2002".to_string()]);
}

#[test]
fn single_target_assignment_is_rejected_e2002() {
    let r = check(&with_main(
        "    var p int\n    p = minmax(3, 9)\n    fmt.println(p)\n",
    ));
    assert_eq!(codes(&r), vec!["HYB-E2002".to_string()]);
}

#[test]
fn parenthesized_call_is_still_a_multi_value() {
    let r = check(&with_main("    p := (minmax(3, 9))\n    fmt.println(p)\n"));
    assert_eq!(codes(&r), vec!["HYB-E2002".to_string()]);
}

#[test]
fn binding_every_result_is_accepted() {
    // The fix-it shape: destructuring is the legal form (T23 "ok" line).
    let r = check(&with_main("    lo, hi := minmax(3, 9)\n    fmt.println(\"{lo} {hi}\")\n    lo, hi = minmax(hi, lo)\n    fmt.println(\"{lo} {hi}\")\n"));
    assert!(
        codes(&r).is_empty(),
        "unexpected errors: {:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn single_result_function_is_not_affected() {
    let r =
        check("fn one() int { return 1 }\n\nfn main() {\n    x := one()\n    fmt.println(x)\n}\n");
    assert!(
        codes(&r).is_empty(),
        "unexpected errors: {:?}",
        r.diagnostics
    );
}

#[test]
fn local_binding_shadowing_the_fn_name_stays_permissive() {
    // A local closure named like the top-level fn is not a static call —
    // its result count is not known here, so no E2002 (no false rejects).
    let r = check(&with_main(
        "    minmax := fn(a int, b int) int { return a + b }\n    p := minmax(3, 9)\n    fmt.println(p)\n",
    ));
    assert!(
        !codes(&r).iter().any(|c| c == "HYB-E2002"),
        "shadowed callee must not raise E2002: {:?}",
        r.diagnostics
    );
}

// ---------------------------------------------------------------------------
// Issue #380: HYB-E2002 (T23) across fn-value bindings, closures, user
// methods, indirect callees, and foreign calls
// ---------------------------------------------------------------------------

#[test]
fn fn_value_binding_and_alias_chain_rejected_e2002() {
    // 1. Direct fn-value binding (`mm := minmax; p := mm(3, 9)`)
    let r1 = check(&with_main(
        "    mm := minmax\n    p := mm(3, 9)\n    fmt.println(p)\n",
    ));
    assert_eq!(codes(&r1), vec!["HYB-E2002".to_string()]);
    assert_eq!(r1.verdict, Verdict::Reject);

    // 2. Multi-hop alias (`mm := minmax; m2 := mm; p := m2(3, 9)`)
    let r2 = check(&with_main(
        "    mm := minmax\n    m2 := mm\n    p := m2(3, 9)\n    fmt.println(p)\n",
    ));
    assert_eq!(codes(&r2), vec!["HYB-E2002".to_string()]);

    // 3. `var p int; p = mm(3, 9)` and `var p = mm(3, 9)`
    let r3 = check(&with_main(
        "    mm := minmax\n    var p int\n    p = mm(3, 9)\n    fmt.println(p)\n",
    ));
    assert_eq!(codes(&r3), vec!["HYB-E2002".to_string()]);

    let r4 = check(&with_main(
        "    mm := minmax\n    var p = mm(3, 9)\n    fmt.println(p)\n",
    ));
    assert_eq!(codes(&r4), vec!["HYB-E2002".to_string()]);

    // 4. Type-aliased function binding (`type pair_fn = fn(a int, b int) int int`)
    let r5 = check(
        "type pair_fn = fn(a int, b int) int int\n\
         const _anchor = 0\n\
         fn minmax(a int, b int) (int, int) { return a, b }\n\
         fn main() {\n\
             var mm pair_fn = minmax\n\
             p := mm(3, 9)\n\
             fmt.println(p)\n\
         }\n",
    );
    assert_eq!(codes(&r5), vec!["HYB-E2002".to_string()]);
}

#[test]
fn declared_and_inline_closures_and_closure_bodies_rejected_e2002() {
    // 1. Declared closure variable returning 2 values bound to 1 name
    let r1 = check(
        "fn main() {\n\
             mm := fn(a int, b int) (int, int) { return a, b }\n\
             p := mm(3, 9)\n\
             fmt.println(p)\n\
         }\n",
    );
    assert_eq!(codes(&r1), vec!["HYB-E2002".to_string()]);

    // 2. Inline closure call returning 2 values bound to 1 name
    let r2 = check(
        "fn main() {\n\
             p := (fn() (int, int) { return 1, 2 })()\n\
             fmt.println(p)\n\
         }\n",
    );
    assert_eq!(codes(&r2), vec!["HYB-E2002".to_string()]);

    // 3. Multi-value bind inside a closure body (direct top-level fn & captured fn-value)
    let r3 = check(&with_main(
        "    mm := minmax\n\
             cb := fn() {\n\
                 p := mm(3, 9)\n\
                 fmt.println(p)\n\
             }\n\
             cb()\n",
    ));
    assert_eq!(codes(&r3), vec!["HYB-E2002".to_string()]);
}

#[test]
fn user_method_and_indirect_fn_callees_rejected_e2002() {
    // 1. User-defined struct method returning 2 values bound to 1 name
    let r1 = check(
        "struct pair_box { a int\n b int }\n\
         fn (p pair_box) unpack() (int, int) { return p.a, p.b }\n\
         fn main() {\n\
             b := pair_box{a: 3, b: 9}\n\
             x := b.unpack()\n\
             fmt.println(x)\n\
         }\n",
    );
    assert_eq!(codes(&r1), vec!["HYB-E2002".to_string()]);

    // 2. Struct field holding a 2-result fn value (`h.mm(3, 9)`)
    let r2 = check(
        "struct holder { mm fn(a int, b int) int int }\n\
         fn minmax(a int, b int) (int, int) { return a, b }\n\
         fn main() {\n\
             h := holder{mm: minmax}\n\
             p := h.mm(3, 9)\n\
             fmt.println(p)\n\
         }\n",
    );
    assert_eq!(codes(&r2), vec!["HYB-E2002".to_string()]);

    // 3. Slice element holding a 2-result fn value (`fns[0](3, 9)`)
    let r3 = check(
        "fn minmax(a int, b int) (int, int) { return a, b }\n\
         fn main() {\n\
             fns := []fn(a int, b int) int int{minmax}\n\
             p := fns[0](3, 9)\n\
             fmt.println(p)\n\
         }\n",
    );
    assert_eq!(codes(&r3), vec!["HYB-E2002".to_string()]);

    // 4. Higher-order function returning a 2-result fn value (`get_mm()(3, 9)`)
    let r4 = check(
        "fn minmax(a int, b int) (int, int) { return a, b }\n\
         fn get_mm() fn(a int, b int) int int { return minmax }\n\
         fn main() {\n\
             p := get_mm()(3, 9)\n\
             fmt.println(p)\n\
         }\n",
    );
    assert_eq!(codes(&r4), vec!["HYB-E2002".to_string()]);

    // 5. Parallel define with a 2-result call in a 1-value slot (`a, b := minmax(3, 9), 1`)
    let r5 = check(&with_main(
        "    a, b := minmax(3, 9), 1\n    fmt.println(\"{a} {b}\")\n",
    ));
    assert_eq!(codes(&r5), vec!["HYB-E2002".to_string()]);
}

#[test]
fn foreign_multi_value_calls_rejected_e2002() {
    let r1 = check(
        "import \"strconv\"\n\
         fn main() {\n\
             n := strconv.parse_int(\"123\", 10, 64)\n\
             fmt.println(n)\n\
         }\n",
    );
    assert_eq!(codes(&r1), vec!["HYB-E2002".to_string()]);

    let r2 = check(
        "import \"strings\"\n\
         fn main() {\n\
             before, after := strings.cut(\"a:b\", \":\")\n\
             fmt.println(\"{before} {after}\")\n\
         }\n",
    );
    assert_eq!(codes(&r2), vec!["HYB-E2002".to_string()]);
}

#[test]
fn positive_controls_destructuring_try_unwrap_and_rebind_accept() {
    // 1. Destructuring fn-value and foreign calls
    let r1 = check(
        "import \"strconv\"\n\
         fn minmax(a int, b int) (int, int) { return a, b }\n\
         fn main() {\n\
             mm := minmax\n\
             a, b := mm(3, 9)\n\
             n, err := strconv.parse_int(\"123\", 10, 64)\n\
             s := strconv.atoi(\"42\")?\n\
             fmt.println(\"{a} {b} {n} {s}\")\n\
             _ = err\n\
         }\n",
    );
    assert!(codes(&r1).is_empty(), "unexpected errors: {:?}", r1.diagnostics);
    assert_eq!(r1.verdict, Verdict::Accept);

    // 2. Rebinding a 2-result fn variable to a 1-result fn is itself a Go
    //    type error (`cannot use plus (value of type func(a int, b int) int)
    //    as func(a int, b int) (int, int) value in assignment`), so it must
    //    reject under HYB-E1002 rather than reach `go build`. This control
    //    used to assert ACCEPT; the program never built (#1599).
    let r2 = check(
        "fn minmax(a int, b int) (int, int) { return a, b }\n\
         fn plus(a int, b int) int { return a + b }\n\
         fn main() {\n\
             mm := minmax\n\
             mm = plus\n\
             p := mm(3, 9)\n\
             fmt.println(p)\n\
         }\n",
    );
    assert!(
        codes(&r2).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for the mismatched fn rebind, got {:?}",
        r2.diagnostics
    );
}
