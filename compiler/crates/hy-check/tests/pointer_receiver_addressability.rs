//! Regression coverage for #656: accepted *T method calls must build in Go.
//! Go's implicit address-of on a value is permitted only for addressable
//! operands. An index into a map, a call result, or a composite literal
//! does not provide a writable receiver slot.
use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check_body(body: &str) -> hy_check::CheckResult {
    let src = format!(
        "struct counter {{ n int }}\n\
         fn (c *counter) bump() {{ c.n += 1 }}\n\
         fn (c counter) read() int {{ return c.n }}\n\
         fn fresh() counter {{ return counter{{n: 0}} }}\n\
         fn main() {{\n{}\n}}\n",
        body
    );
    let file = parse_file("pointer_receivers.hy", &src)
        .unwrap_or_else(|errors| panic!("fixture must parse: {errors:?}\n{src}"));
    check_file(&file)
}

fn assert_nonaddressable(body: &str) {
    let result = check_body(body);
    assert_eq!(result.verdict, Verdict::Reject, "{:?}", result.diagnostics);
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E1002" && d.tier0_rule.as_deref() == Some("BUILDABLE")),
        "missing addressability diagnostic: {:?}",
        result.diagnostics
    );
}

fn assert_addressable(body: &str) {
    let result = check_body(body);
    assert!(
        !result.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "legal receiver rejected: {:?}",
        result.diagnostics
    );
    assert_eq!(result.verdict, Verdict::Accept, "{:?}", result.diagnostics);
}

#[test]
fn map_element_cannot_call_pointer_receiver_method() {
    assert_nonaddressable("m := map[string]counter{\"a\": counter{n: 0}}\nm[\"a\"].bump()");
}

#[test]
fn function_result_cannot_call_pointer_receiver_method() {
    assert_nonaddressable("fresh().bump()");
}

#[test]
fn composite_literal_cannot_call_pointer_receiver_method() {
    assert_nonaddressable("counter{n: 5}.bump()");
}

#[test]
fn ordinary_local_is_addressable() {
    assert_addressable("c := counter{n: 0}\nc.bump()");
}

#[test]
fn slice_element_is_addressable() {
    assert_addressable("xs := []counter{counter{n: 0}}\nxs[0].bump()");
}

#[test]
fn explicit_map_slot_loan_is_accepted() {
    assert_addressable("m := map[string]counter{\"a\": counter{n: 0}}\np := &m[\"a\"]\np.bump()");
}

#[test]
fn value_receivers_work_on_nonaddressable_values() {
    assert_addressable("m := map[string]counter{\"a\": counter{n: 0}}\na := m[\"a\"].read()\nb := fresh().read()\nc := counter{n: 2}.read()\n_ = a\n_ = b\n_ = c");
}

#[test]
fn parenthesized_method_callee_cannot_hide_nonaddressable_receiver() {
    assert_nonaddressable("(fresh().bump)()");
}

#[test]
fn task_call_cannot_hide_nonaddressable_receiver() {
    assert_nonaddressable("managed(\"receiver regression\", \"other\") { task fresh().bump() }");
}

#[test]
fn closure_body_cannot_hide_nonaddressable_receiver() {
    assert_nonaddressable("f := fn() { fresh().bump() }\nf()");
}

#[test]
fn channel_receive_is_not_addressable() {
    assert_nonaddressable("ch := make(chan counter, 1)\ndefer (<-ch).bump()");
}

#[test]
fn deferred_call_cannot_hide_nonaddressable_receiver() {
    assert_nonaddressable("defer fresh().bump()");
}

#[test]
fn temporary_pointer_can_call_pointer_receiver_method() {
    assert_addressable("new(counter).bump()");
}

#[test]
fn temporary_slice_element_is_addressable() {
    assert_addressable("[]counter{counter{n: 0}}[0].bump()");
}

#[test]
fn file_level_composite_binding_is_addressable_for_read_only_method() {
    let src = "struct counter { n int }\n\
               fn (c *counter) read() int { return c.n }\n\
               const g = counter{n: 1}\n\
               fn main() { _ = g.read() }\n";
    let file = parse_file("global_pointer_receiver.hy", src).expect("fixture must parse");
    let result = check_file(&file);
    assert_eq!(result.verdict, Verdict::Accept, "{:?}", result.diagnostics);
}

#[test]
fn bound_method_value_requires_addressable_receiver() {
    assert_nonaddressable("f := fresh().bump\nf()");
}

#[test]
fn bound_pointer_method_on_local_is_accepted() {
    assert_addressable("c := counter{n: 0}\nf := c.bump\nf()");
}

#[test]
fn bound_pointer_method_on_temporary_pointer_is_accepted() {
    assert_addressable("f := new(counter).bump\nf()");
}

#[test]
fn bound_value_method_on_temporary_value_is_accepted() {
    assert_addressable("f := fresh().read\n_ = f()");
}
