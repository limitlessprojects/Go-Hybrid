//! managed_newline_injection_1917.rs — regression for issue #1917.
//!
//! The emitter interpolates the managed reason and tickets into a single-line
//! `// managed [category] (reason) [tickets]` comment. `audit_managed_block`
//! only checked that the reason was non-empty, so a newline or carriage
//! return escaped the comment and injected Go code; blank tickets injected
//! blank lines.

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_reject(src: &str) {
    let found = codes(src);
    assert!(
        found.iter().any(|code| code == "HYB-E0003"),
        "expected HYB-E0003, got {found:?} for:\n{src}"
    );
}

fn assert_accept(src: &str) {
    let found = codes(src);
    assert!(
        found.is_empty(),
        "expected acceptance, got {found:?} for:\n{src}"
    );
}

#[test]
fn rejects_newline_in_managed_reason() {
    assert_reject(
        "fn main() {\n    managed(\"why\ninjected()\", \"mutex\") { x := 1\n        _ = x }\n}\n",
    );
}

#[test]
fn rejects_carriage_return_in_managed_reason() {
    assert_reject(
        "fn main() {\n    managed(\"why\rinjected()\", \"mutex\") { x := 1\n        _ = x }\n}\n",
    );
}

#[test]
fn rejects_newline_in_managed_ticket() {
    assert_reject(
        "fn main() {\n    managed(\"guarded\", \"mutex\", \"t1\ninjected()\") { x := 1\n        _ = x }\n}\n",
    );
}

#[test]
fn rejects_blank_managed_ticket() {
    assert_reject(
        "fn main() {\n    managed(\"guarded\", \"mutex\", \"  \") { x := 1\n        _ = x }\n}\n",
    );
}

#[test]
fn accepts_single_line_reason_and_tickets() {
    assert_accept(
        "fn main() {\n    managed(\"guarded by mutex\", \"mutex\", \"t1\", \"t2\") { x := 1\n        _ = x }\n}\n",
    );
}
