//! Tests for issue #1480: range-element mutation on struct elements
//! (`for x in xs { x.n += 10 }`, pointer-receiver method calls) must reject
//! fail-closed because Go's `for _, x := range xs` copy semantics silently
//! discards all modifications to the copy `x` at runtime.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check_body(body: &str) -> hy_check::CheckResult {
    let src = format!(
        "struct cell {{ n int }}\n\
         fn (c *cell) bump() {{ c.n += 10 }}\n\
         fn (c cell) read() int {{ return c.n }}\n\
         fn main() {{\n{}\n}}\n",
        body
    );
    let file = parse_file("range_element_mutation.hy", &src)
        .unwrap_or_else(|errors| panic!("fixture must parse: {errors:?}\n{src}"));
    check_file(&file)
}

fn assert_rejects(body: &str, expected_sub: &str) {
    let result = check_body(body);
    assert_eq!(result.verdict, Verdict::Reject, "{:?}", result.diagnostics);
    let found = result.diagnostics.iter().any(|d| {
        d.code == "HYB-E1002"
            && d.tier0_rule.as_deref() == Some("BUILDABLE")
            && d.message.contains(expected_sub)
    });
    assert!(
        found,
        "missing expected diagnostic containing `{expected_sub}` in: {:?}",
        result.diagnostics
    );
}

fn assert_accepts(body: &str) {
    let result = check_body(body);
    assert!(
        !result.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "legal range construct rejected: {:?}",
        result.diagnostics
    );
    assert_eq!(result.verdict, Verdict::Accept, "{:?}", result.diagnostics);
}

#[test]
fn form_a_direct_field_write_one_var_rejects() {
    // Form A from #1480:
    assert_rejects(
        "xs := []cell{cell{n: 1}, cell{n: 2}}\n\
         for x in xs {\n\
             x.n += 10\n\
         }\n\
         _ = xs",
        "cannot mutate field of range element `x.n`",
    );
}

#[test]
fn form_b_direct_field_write_two_var_rejects() {
    // Form B from #1480:
    assert_rejects(
        "xs := []cell{cell{n: 1}}\n\
         for i, x in xs {\n\
             x.n += 10\n\
             _ = i\n\
         }\n\
         _ = xs",
        "cannot mutate field of range element `x.n`",
    );
}

#[test]
fn form_c_pointer_receiver_method_rejects() {
    // Form C from #1480:
    assert_rejects(
        "xs := []cell{cell{n: 1}}\n\
         for x in xs {\n\
             x.bump()\n\
         }\n\
         _ = xs",
        "cannot call pointer-receiver method `bump` on range element `x`",
    );
}

#[test]
fn form_c_pointer_receiver_method_two_var_rejects() {
    assert_rejects(
        "xs := []cell{cell{n: 1}}\n\
         for i, x in xs {\n\
             x.bump()\n\
             _ = i\n\
         }\n\
         _ = xs",
        "cannot call pointer-receiver method `bump` on range element `x`",
    );
}

#[test]
fn field_assign_operators_reject() {
    assert_rejects(
        "xs := []cell{cell{n: 1}}\n\
         for x in xs {\n\
             x.n = 20\n\
         }\n\
         _ = xs",
        "cannot mutate field of range element `x.n`",
    );
    assert_rejects(
        "xs := []cell{cell{n: 1}}\n\
         for x in xs {\n\
             x.n++\n\
         }\n\
         _ = xs",
        "cannot mutate field of range element `x.n`",
    );
    assert_rejects(
        "xs := []cell{cell{n: 1}}\n\
         for x in xs {\n\
             x.n--\n\
         }\n\
         _ = xs",
        "cannot mutate field of range element `x.n`",
    );
}

#[test]
fn value_receiver_on_range_element_is_accepted() {
    // Value receivers only read the struct copy; they do not mutate or auto-address.
    assert_accepts(
        "xs := []cell{cell{n: 1}}\n\
         sum := 0\n\
         for x in xs {\n\
             sum += x.read()\n\
         }\n\
         _ = sum",
    );
}

#[test]
fn pointer_slice_iteration_allows_mutation() {
    // Iterating []*cell yields pointer copies; dereferencing mutates heap storage.
    assert_accepts(
        "xs := []*cell{&cell{n: 1}}\n\
         for x in xs {\n\
             x.n += 10\n\
             x.bump()\n\
         }\n\
         _ = xs",
    );
}

#[test]
fn index_based_iteration_allows_mutation() {
    // The official fix-it: mutate collection elements through indexing.
    assert_accepts(
        "xs := []cell{cell{n: 1}}\n\
         for i in 0..len(xs) {\n\
             xs[i].n += 10\n\
             xs[i].bump()\n\
         }\n\
         _ = xs",
    );
}

#[test]
fn ordinary_local_struct_allows_mutation() {
    assert_accepts(
        "c := cell{n: 1}\n\
         c.n += 10\n\
         c.bump()\n\
         _ = c",
    );
}
