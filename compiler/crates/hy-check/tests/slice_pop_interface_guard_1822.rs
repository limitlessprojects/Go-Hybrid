//! Regression tests for #1822 / #1979: `any` / interface `.pop()` rejection.

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn rejects_pop_on_any_interface_and_struct_without_pop_method() {
    let found = codes(
        "struct point {\n    x int\n    y int\n}\n\nfn main() {\n    var pt point\n    _ = pt.pop()\n}\n",
    );
    assert!(
        found.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 on struct without pop method, got {found:?}"
    );

    let found_ptr = codes(
        "struct point {\n    x int\n    y int\n}\n\nfn main() {\n    var pt point\n    var ptr *point = &pt\n    _ = ptr.pop()\n}\n",
    );
    assert!(
        found_ptr.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 on pointer without pop method, got {found_ptr:?}"
    );

    let found_any = codes("fn main() {\n    var a any\n    _ = a.pop()\n}\n");
    assert!(
        found_any.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 on any interface .pop(), got {found_any:?}"
    );
}
