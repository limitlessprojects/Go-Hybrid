//! Issue #2771: known array lengths participate in operand type compatibility.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn assert_rejects(src: &str) {
    let file = parse_file("array-length.hy", src).expect("regression must parse");
    let result = check_file(&file);
    assert_eq!(result.verdict, Verdict::Reject, "accepted:\n{src}");
    assert!(
        result.diagnostics.iter().any(|diagnostic| {
            diagnostic.severity == Severity::Error
                && diagnostic.code == "HYB-E1002"
                && diagnostic.tier0_rule.as_deref() == Some("BUILDABLE")
        }),
        "expected BUILDABLE/HYB-E1002, got {:?} for:\n{src}",
        result.diagnostics
    );
}

fn assert_accepts(src: &str) {
    let file = parse_file("array-length.hy", src).expect("control must parse");
    let result = check_file(&file);
    assert_eq!(
        result.verdict,
        Verdict::Accept,
        "rejected valid array use: {:?}\n{src}",
        result.diagnostics
    );
    assert!(result.diagnostics.is_empty(), "{:?}", result.diagnostics);
}

#[test]
fn rejects_equality_and_inequality_with_different_lengths() {
    for operator in ["==", "!="] {
        assert_rejects(&format!(
            "fn main() {{\n var a [3]int\n var b [4]int\n _ = a {operator} b\n}}\n"
        ));
    }
}

#[test]
fn rejects_assignment_with_different_lengths() {
    assert_rejects("fn main() {\n var a [3]int\n var b [4]int\n b = a\n _ = b\n}\n");
}

#[test]
fn rejects_typed_declaration_with_different_lengths() {
    assert_rejects("fn main() {\n var a [3]int\n var b [4]int = a\n _ = b\n}\n");
}

#[test]
fn rejects_argument_with_different_length() {
    assert_rejects(
        "fn take(x [4]int) int {\n return x[0]\n}\nfn main() {\n var a [3]int\n _ = take(a)\n}\n",
    );
}

#[test]
fn rejects_return_with_different_length() {
    assert_rejects("fn make_array() [4]int {\n var a [3]int\n return a\n}\n");
}

#[test]
fn rejects_nested_array_length_mismatches() {
    for (left, right) in [("[2][3]int", "[2][4]int"), ("[2][3]int", "[3][3]int")] {
        assert_rejects(&format!(
            "fn main() {{\n var a {left}\n var b {right}\n b = a\n _ = b\n}}\n"
        ));
    }
}

#[test]
fn rejects_aliases_with_different_lengths() {
    assert_rejects(
        "type small = [3]int\ntype large = [4]int\nfn main() {\n var a small\n var b large\n _ = a == b\n}\n",
    );
}

#[test]
fn rejects_literal_and_inferred_length_mismatches() {
    for left in ["[3]int{}", "[...]int{1, 2, 3}"] {
        assert_rejects(&format!("fn main() {{\n _ = {left} == [4]int{{}}\n}}\n"));
    }
}

#[test]
fn rejects_different_constant_expression_lengths() {
    assert_rejects("const n = 3\nfn main() {\n var a [n]int\n var b [n + 1]int\n _ = a == b\n}\n");
}

#[test]
fn rejects_zero_length_against_nonzero_length() {
    assert_rejects("fn main() {\n var a [0]int\n var b [1]int\n _ = a != b\n}\n");
}

#[test]
fn rejects_array_type_conversion_with_different_length() {
    assert_rejects("type large = [4]int\nfn main() {\n var a [3]int\n _ = large(a)\n}\n");
}

#[test]
fn rejects_generic_struct_array_field_length_mismatches() {
    assert_rejects(
        "struct holder[t] {\n value [3]t\n}\nfn main() {\n var h holder[int]\n var a [4]int\n _ = h.value == a\n}\n",
    );
    assert_rejects(
        "struct box[t] {\n value t\n}\nfn main() {\n var h box[[3]int]\n var a [4]int\n _ = h.value == a\n}\n",
    );
}

#[test]
fn rejects_method_argument_with_different_length() {
    assert_rejects(
        "struct receiver {}\nfn (r receiver) take(x [4]int) int {\n return x[0]\n}\nfn main() {\n var r receiver\n var a [3]int\n _ = r.take(a)\n}\n",
    );
}

#[test]
fn accepts_equal_lengths_in_assignments_arguments_returns_and_comparisons() {
    assert_accepts(
        "fn take(x [3]int) [3]int {\n return x\n}\nfn main() {\n var a [3]int\n var b [3]int = a\n b = take(a)\n _ = a == b\n _ = a != b\n}\n",
    );
}

#[test]
fn accepts_equivalent_constant_lengths_and_shadowed_constants() {
    assert_accepts(
        "const n = 2\nfn main() {\n var a [n + 1]int\n var b [3]int = a\n _ = a == b\n}\n",
    );
    assert_accepts(
        "const n = 2\nfn main() {\n const n = 3\n var a [n]int\n var b [3]int = a\n _ = a == b\n}\n",
    );
}

#[test]
fn accepts_matching_inferred_lengths_and_array_operations() {
    assert_accepts(
        "fn take(x [3]int) int {\n return x[0]\n}\nfn main() {\n a := [...]int{1, 2, 3}\n var b [3]int = a\n _ = a == b\n _ = take(a)\n _ = len(a)\n _ = cap(a)\n _ = a[1:]\n _ = a[0]\n for i, v in a {\n _ = i\n _ = v\n }\n}\n",
    );
}

#[test]
fn accepts_generic_array_fields_with_matching_or_unresolved_length() {
    for length in ["3", "n", "n + 1"] {
        let expected = if length == "n + 1" { 4 } else { 3 };
        assert_accepts(&format!(
            "const n = 3\nstruct box[t] {{\n value t\n}}\ntype matrix = [2][{expected}]int\nfn main() {{\n var h box[[2][{length}]int]\n _ = matrix(h.value)\n}}\n"
        ));
    }
}

#[test]
fn still_rejects_conversion_between_different_function_element_types() {
    assert_rejects(
        "type wanted = []fn() int\nfn main() {\n var values []fn() string\n _ = wanted(values)\n}\n",
    );
}

#[test]
fn rejects_copy_between_slices_of_different_array_types() {
    assert_rejects("fn main() {\n var dst [][3]int\n var src [][4]int\n _ = copy(dst, src)\n}\n");
}

#[test]
fn accepts_copy_between_slices_of_equal_array_types() {
    assert_accepts("fn main() {\n var dst [][3]int\n var src [][3]int\n _ = copy(dst, src)\n}\n");
}
