//! issue_2989_set_operators.rs — operators (`+ - * < !`) and the builtins
//! `append` / `copy` / `min` / `max` on a `set[T]` resolve it to
//! `OpKind::Unknown`, which those gates treat as fail-open. A set lowers to a
//! Go struct, so none of these build (Row 13 BUILDABLE).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("repro.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn assert_rejected(src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, diags: {:?}\nsrc:\n{src}", r.diagnostics);
}

#[test]
fn binary_arithmetic_on_a_set_is_rejected() {
    assert_rejected("fn main() {\n    var s set[int]\n    _ = s + 1\n}\n");
    assert_rejected("fn main() {\n    var s set[int]\n    _ = 1 * s\n}\n");
}

#[test]
fn ordered_comparison_on_a_set_is_rejected() {
    assert_rejected("fn main() {\n    var s set[int]\n    _ = s < 1\n}\n");
}

#[test]
fn unary_neg_on_a_set_is_rejected() {
    assert_rejected("fn main() {\n    var s set[int]\n    _ = -s\n}\n");
}

#[test]
fn unary_not_on_a_set_is_rejected() {
    assert_rejected("fn main() {\n    var s set[int]\n    _ = !s\n}\n");
}

#[test]
fn append_on_a_set_is_rejected() {
    assert_rejected("fn main() {\n    var s set[int]\n    _ = append(s, 1)\n}\n");
}

#[test]
fn copy_from_a_set_is_rejected() {
    assert_rejected("fn main() {\n    var s set[int]\n    var sl []int\n    _ = copy(sl, s)\n}\n");
}

#[test]
fn min_of_sets_is_rejected() {
    assert_rejected("fn main() {\n    var s1 set[int]\n    var s2 set[int]\n    _ = min(s1, s2)\n}\n");
}

#[test]
fn valid_set_operations_stay_accepted() {
    let r = check("fn main() {\n    s := set[int]{}\n    s.insert(1)\n    fmt.println(s.contains(1))\n}\n");
    assert_eq!(r.verdict, Verdict::Accept, "valid set ops must stay accepted, diags: {:?}", r.diagnostics);
}
