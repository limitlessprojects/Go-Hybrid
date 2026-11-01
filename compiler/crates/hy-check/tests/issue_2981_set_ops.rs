//! issue_2981_set_ops.rs — indexing, slicing, `cap`, `close` and `clear` on a
//! `set[T]` resolve it to `OpKind::Unknown`, which the operand checks treat as
//! fail-open. A set lowers to a Go struct, so none of these build (Row 13).

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
fn indexing_a_set_is_rejected() {
    assert_rejected("fn main() {\n    var s set[int]\n    _ = s[0]\n}\n");
}

#[test]
fn slicing_a_set_is_rejected() {
    assert_rejected("fn main() {\n    var s set[int]\n    _ = s[0:1]\n}\n");
}

#[test]
fn cap_of_a_set_is_rejected() {
    assert_rejected("fn main() {\n    var s set[int]\n    _ = cap(s)\n}\n");
}

#[test]
fn close_of_a_set_is_rejected() {
    assert_rejected("fn main() {\n    var s set[int]\n    close(s)\n}\n");
}

#[test]
fn clear_of_a_set_is_rejected() {
    assert_rejected("fn main() {\n    var s set[int]\n    clear(s)\n}\n");
}

#[test]
fn valid_set_operations_stay_accepted() {
    let r = check("fn main() {\n    s := set[int]{}\n    s.insert(1)\n    fmt.println(s.contains(1))\n}\n");
    assert_eq!(r.verdict, Verdict::Accept, "valid set ops must stay accepted, diags: {:?}", r.diagnostics);
}
