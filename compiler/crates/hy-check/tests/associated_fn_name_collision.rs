//! Associated functions lower `Type.name` to the package-level Go identifier
//! `Type_name`. That emitted identifier must not collide with another package
//! declaration merely because the Hybrid source spellings differ.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).expect("test snippet must parse");
    check_file(&ast)
}

fn assert_rejects(src: &str) {
    let result = check(src);
    assert_eq!(result.verdict, Verdict::Reject, "src:\n{src}");
    assert!(
        result
            .diagnostics
            .iter()
            .any(|d| d.severity == Severity::Error && d.code == "HYB-E1003"),
        "expected HYB-E1003, got {:?}",
        result.diagnostics
    );
}

fn assert_accepts(src: &str) {
    let result = check(src);
    assert_eq!(
        result.verdict,
        Verdict::Accept,
        "control must stay accepted: {:?}",
        result.diagnostics
    );
}

const STRUCT: &str = "struct counter {\n    n int\n}\n";
const ASSOC: &str = "fn counter.make() counter {\n    return counter{n: 1}\n}\n";
const PLAIN: &str = "fn counter_make() int {\n    return 2\n}\n";
const MAIN: &str = "fn main() {\n    fmt.println(\"ok\")\n}\n";

#[test]
fn associated_then_plain_emitted_name_collision_rejects() {
    assert_rejects(&format!("{STRUCT}{ASSOC}{PLAIN}{MAIN}"));
}

#[test]
fn plain_then_associated_emitted_name_collision_rejects() {
    assert_rejects(&format!("{STRUCT}{PLAIN}{ASSOC}{MAIN}"));
}

#[test]
fn associated_name_colliding_with_struct_rejects() {
    assert_rejects(&format!(
        "{STRUCT}struct counter_make {{\n    n int\n}}\n{ASSOC}{MAIN}"
    ));
}

#[test]
fn associated_function_without_collision_stays_accepted() {
    assert_accepts(&format!("{STRUCT}{ASSOC}{MAIN}"));
}

#[test]
fn plain_underscore_function_without_collision_stays_accepted() {
    assert_accepts(&format!("{PLAIN}{MAIN}"));
}
