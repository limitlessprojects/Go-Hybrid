use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    check_file(&parse_file("test.hy", src).expect("parses")).diagnostics.into_iter().map(|d| d.code).collect()
}
fn assert_e1002(src: &str) { assert_eq!(codes(src), vec!["HYB-E1002".to_string()], "for:\n{src}"); }
fn assert_clean(src: &str) { let cs = codes(src); assert!(cs.is_empty(), "got {cs:?} for:\n{src}"); }

#[test]
fn short_defined_int_returned_as_float64_is_rejected() {
    assert_e1002("fn f() float64 { n := 3\n return n }\nfn main() { fmt.println(\"{f()}\") }\n");
}
#[test]
fn int_param_returned_as_float64_is_rejected() {
    assert_e1002("fn f(n int) float64 { return n }\nfn main() { fmt.println(\"{f(2)}\") }\n");
}
#[test]
fn int_returned_as_int64_is_rejected() {
    assert_e1002("fn f() int64 { n := 3\n return n }\nfn main() { fmt.println(\"{f()}\") }\n");
}
#[test]
fn float64_returned_as_float32_is_rejected() {
    assert_e1002("fn f() float32 { var x float64 = 1.5\n return x }\nfn main() { fmt.println(\"{f()}\") }\n");
}
#[test]
fn typed_const_returned_as_other_width_is_rejected() {
    assert_e1002("fn f() int { const c int64 = 3\n return c }\nfn main() { fmt.println(\"{f()}\") }\n");
}
#[test]
fn typed_var_into_error_tail_short_return_is_rejected() {
    assert_e1002("fn f() (float64, error) { n := 3\n return n, nil }\nfn main() { a, _ := f()\n fmt.println(\"{a}\") }\n");
}
#[test]
fn call_result_of_other_numeric_type_is_rejected() {
    assert_e1002("fn g() int { return 1 }\nfn f() float64 { return g() }\nfn main() { fmt.println(\"{f()}\") }\n");
}
#[test]
fn conversion_to_the_wrong_width_is_rejected() {
    assert_e1002("fn f(n int) int { return int64(n) }\nfn main() { fmt.println(\"{f(1)}\") }\n");
}
#[test]
fn multi_value_binding_keeps_callee_result_types() {
    assert_e1002("fn pair() (int, int) { return 1, 2 }\nfn f() float64 { a, b := pair()\n fmt.println(\"{a}\")\n return b }\nfn main() { fmt.println(\"{f()}\") }\n");
}
#[test]
fn closure_body_return_is_checked_against_the_closure_signature() {
    assert_e1002("fn f() float64 { h := fn() float64 { x := 1\n return x }\n return h() }\nfn main() { fmt.println(\"{f()}\") }\n");
}
#[test]
fn named_result_signature_rejects_other_width() {
    assert_e1002("fn f() (r int) { var x int64 = 1\n return x }\nfn main() { fmt.println(\"{f()}\") }\n");
}
#[test]
fn typed_arithmetic_keeps_its_operand_type() {
    assert_e1002("fn f() float64 { n := 3\n return n * 2 }\nfn main() { fmt.println(\"{f()}\") }\n");
    assert_e1002("fn f(n int) int64 { return -n }\nfn main() { fmt.println(\"{f(1)}\") }\n");
    assert_e1002("fn f(x int64, n int) int { return x << n }\nfn main() { fmt.println(\"{f(1, 2)}\") }\n");
}
#[test]
fn untyped_constants_stay_assignable_to_any_numeric_result() {
    assert_clean("fn f() float64 { return 3 }\nfn main() { fmt.println(\"{f()}\") }\n");
    assert_clean("fn f() float64 { const c = 3\n return c }\nfn main() { fmt.println(\"{f()}\") }\n");
    assert_clean("const c = 3\nfn f() float64 { return c }\nfn main() { fmt.println(\"{f()}\") }\n");
    assert_clean("fn f(n int) float64 { return 2.0 * 3 }\nfn main() { fmt.println(\"{f(1)}\") }\n");
}
#[test]
fn explicit_conversions_are_accepted() {
    assert_clean("fn f(n int) float64 { return float64(n) }\nfn main() { fmt.println(\"{f(2)}\") }\n");
    assert_clean("fn f() float64 { var x = 2\n y := float64(x)\n return y }\nfn main() { fmt.println(\"{f()}\") }\n");
}
#[test]
fn matching_types_and_go_identities_are_accepted() {
    assert_clean("fn f() float64 { n := 3.0\n return n }\nfn main() { fmt.println(\"{f()}\") }\n");
    assert_clean("fn f(b byte) uint8 { return b }\nfn g(r rune) int32 { return r }\nfn h(x i64) int64 { return x }\nfn main() { fmt.println(\"{f(1)} {g(2)} {h(3)}\") }\n");
    assert_clean("fn f(n int) int64 { return 1 << n }\nfn g(x int64, n int) int64 { return x << n }\nfn k(x float64) float64 { y := -x\n return y * 2 }\nfn main() { fmt.println(\"{f(1)} {g(1, 2)} {k(1.0)}\") }\n");
}
#[test]
fn inner_shadowing_binding_does_not_inherit_the_outer_type() {
    assert_clean("fn f(k bool) float64 { n := 3\n if k { n := 2.5\n return n }\n return float64(n) }\nfn main() { fmt.println(\"{f(true)}\") }\n");
}
#[test]
fn closures_named_results_and_range_counters_are_accepted() {
    assert_clean("fn f() float64 { h := fn() float64 { x := 1.5\n return x }\n return h() }\nfn main() { fmt.println(\"{f()}\") }\n");
    assert_clean("fn f() (r float64) { r = 2\n return }\nfn main() { fmt.println(\"{f()}\") }\n");
    assert_clean("fn f() int { for i in 0..3 { return i }\n return 0 }\nfn main() { fmt.println(\"{f()}\") }\n");
}
