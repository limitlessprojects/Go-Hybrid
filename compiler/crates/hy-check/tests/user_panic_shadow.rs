//! user_panic_shadow.rs — tests for Go-Hybrid #1035.
//! Verifies that package-level user declarations matching Go prelude builtins (e.g. `fn panic`)
//! are rejected with HYB-E1001 to prevent shadowing runtime prelude calls like `option.unwrap()`.

use hy_check::check_file;
use hy_syntax::parse_file;

fn rejects_with_e1001(src: &str) {
    let file = parse_file("t.hy", src).expect("parse");
    let res = check_file(&file);
    let has_e1001 = res
        .diagnostics
        .iter()
        .any(|d| d.code == "HYB-E1001" && d.severity == hy_diag::Severity::Error);
    assert!(
        has_e1001,
        "expected HYB-E1001 error for source:\n{}\ndiags: {:#?}",
        src, res.diagnostics
    );
}

#[test]
fn package_level_panic_shadow_rejected_issue_1035() {
    rejects_with_e1001(
        "fn panic(msg string) {\n\
             fmt.println(\"user panic: \" + msg)\n\
         }\n\
         fn main() {\n\
             var o option[int]\n\
             fmt.println(o.unwrap())\n\
         }\n"
    );
}

#[test]
fn package_level_len_shadow_rejected_issue_1035() {
    rejects_with_e1001(
        "fn len(xs []int) int {\n\
             return 0\n\
         }\n\
         fn main() {\n\
             _ = len([]int{1})\n\
         }\n"
    );
}
