//! Issue #3467: reject untyped `nil` passed to Go non-nilable call parameters.

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("parses");
    check_file(&ast)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn assert_e1002(src: &str) {
    let cs = codes(src);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for:\n{src}\ngot {cs:?}"
    );
}

#[test]
fn rejects_nil_for_non_nilable_function_parameters() {
    assert_e1002("fn takes_int(x int32) {}\nfn main() {\n    takes_int(nil)\n}\n");
    assert_e1002("fn takes_float(x float64) {}\nfn main() {\n    takes_float(nil)\n}\n");
    assert_e1002("fn takes_string(x string) {}\nfn main() {\n    takes_string(nil)\n}\n");
    assert_e1002("fn takes_bool(x bool) {}\nfn main() {\n    takes_bool(nil)\n}\n");
    assert_e1002(
        "struct point { x int32 }\nfn takes_struct(x point) {}\nfn main() {\n    takes_struct(nil)\n}\n",
    );
    assert_e1002("fn takes_array(x [3]int32) {}\nfn main() {\n    takes_array(nil)\n}\n");
}

#[test]
fn rejects_nil_for_method_and_function_literal_parameters() {
    assert_e1002(
        "struct receiver {}\nfn (r receiver) takes_int(x int32) {}\nfn main() {\n    var r receiver\n    r.takes_int(nil)\n}\n",
    );
    assert_e1002("fn main() {\n    (fn(x int32) {})(nil)\n}\n");
}

#[test]
fn nil_for_nilable_call_parameters_stays_accepted() {
    let cs = codes("fn takes_any(x any) {}\nfn main() {\n    takes_any(nil)\n}\n");
    assert!(cs.is_empty(), "expected no diagnostics, got {cs:?}");
}
