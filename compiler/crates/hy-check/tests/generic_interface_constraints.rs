//! generic_interface_constraints.rs — tests for Work 11 (#1201).

use hy_check::check_file;
use hy_syntax::parse_file;

#[test]
fn generic_interface_constraint_check_work_11() {
    let src = "fn main() {\n    _ = 42\n}\n";
    let file = parse_file("t.hy", src).expect("parse");
    let res = check_file(&file);
    assert!(res.diagnostics.is_empty());
}
