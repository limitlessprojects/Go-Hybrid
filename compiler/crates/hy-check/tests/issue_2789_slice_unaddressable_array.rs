//! #2789: Go spec, Slice expressions — "If the sliced operand is an array,
//! it must be addressable." Composite literals, call results and map index
//! expressions of array type are not addressable; slicing them must be
//! rejected (Go: `cannot slice unaddressable value`).

use hy_check::check_file;
use hy_diag::Diagnostic;
use hy_syntax::parse_file;

fn check(src: &str) -> Vec<Diagnostic> {
    let ast = parse_file("test.hy", src).expect("parses");
    check_file(&ast).diagnostics
}

fn assert_unaddressable(src: &str) {
    let diags = check(src);
    assert!(
        diags
            .iter()
            .any(|d| d.code == "HYB-E1002" && d.message.contains("cannot slice unaddressable array value")),
        "expected unaddressable-slice HYB-E1002, got {:?}",
        diags
    );
}

#[test]
fn slicing_array_literal_rejected() {
    assert_unaddressable(
        "fn main() {\n    s := [3]int{1, 2, 3}[0:2]\n    fmt.println(s)\n}\n",
    );
}

#[test]
fn slicing_parenthesized_array_literal_rejected() {
    assert_unaddressable(
        "fn main() {\n    s := ([3]int{1, 2, 3})[1:]\n    fmt.println(s)\n}\n",
    );
}

#[test]
fn slicing_array_call_result_rejected() {
    assert_unaddressable(
        "fn arr() [3]int {\n    return [3]int{1, 2, 3}\n}\nfn main() {\n    s := arr()[1:]\n    fmt.println(s)\n}\n",
    );
}

#[test]
fn slicing_map_element_array_rejected() {
    assert_unaddressable(
        "fn main() {\n    m := map[string][3]int{\"a\": [3]int{1, 2, 3}}\n    s := m[\"a\"][:2]\n    fmt.println(s)\n}\n",
    );
}

#[test]
fn slicing_addressable_arrays_slices_and_strings_accepted() {
    let src = "struct box {\n    a [3]int\n}\nfn sl() []int {\n    return []int{1, 2, 3}\n}\nfn main() {\n    a := [3]int{1, 2, 3}\n    b := box{a: [3]int{4, 5, 6}}\n    p := &a\n    m := map[string][]int{\"k\": []int{1, 2}}\n    fmt.println(a[0:2], b.a[1:], p[:1], sl()[1:], m[\"k\"][:1], []int{1, 2}[1:], \"abc\"[1:])\n}\n";
    let diags = check(src);
    assert!(
        !diags.iter().any(|d| d.message.contains("cannot slice unaddressable")),
        "addressable operands must be accepted, got {:?}",
        diags
    );
}
