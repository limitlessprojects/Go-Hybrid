//! Regression tests for #1822: declared `pop` method resolution and slice `.pop()` acceptance.

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
fn user_declared_pop_method_and_slice_pop_stay_accepted() {
    let found_method = codes(
        "struct stack {\n    items []int\n}\n\nfn (s *stack) pop() int {\n    return 42\n}\n\nfn main() {\n    var s stack\n    v := s.pop()\n    fmt.println(\"{v}\")\n}\n",
    );
    assert!(
        found_method.is_empty(),
        "expected declared pop method to pass, got {found_method:?}"
    );

    let found_slice = codes(
        "fn main() {\n    s := []int{1, 2, 3}\n    v := s.pop()\n    fmt.println(\"{v}\")\n}\n",
    );
    assert!(
        found_slice.is_empty(),
        "expected slice .pop() to pass, got {found_slice:?}"
    );
}
