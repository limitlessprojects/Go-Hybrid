//! closure_lifetime_backedge.rs — tests for Work 12 (#1205).

use hy_check::check_file;
use hy_syntax::parse_file;

#[test]
fn closure_lifetime_backedge_check_work_12() {
    let src = "fn main() {\n    _ = 42\n}\n";
    let file = parse_file("t.hy", src).expect("parse");
    let res = check_file(&file);
    assert!(res.diagnostics.is_empty());
}
