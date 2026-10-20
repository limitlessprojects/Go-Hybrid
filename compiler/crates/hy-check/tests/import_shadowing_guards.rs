//! unbuildable_declaration.rs — regression for issue #134: `hyb check`
//! used to accept three declaration shapes whose emitted Go can never build:
//!
//!   1. an unused `:=` binding — Go: `declared and not used` (including
//!      `_ := 5` / `_, _ := ...`, Go: `no new variables on left side of :=`);
//!   2. an unused import — Go: `imported and not used`;
//!   3. a repeated name on the left of one `:=` — Go: `x repeated on left
//!      side of :=`.
//!
//! All three now reject fail-closed as HYB-E1002 (rule `BUILDABLE`). The
//! read-set follows Go's own unused-variable check: a mention anywhere in
//! the function counts (closure bodies included; `x++`, `x += 1`,
//! `x[k] = v`, `x.f = v`, `&x` and `_ = x` all count), while a bare `x = v`
//! store does not.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn first_error_code(r: &hy_check::CheckResult) -> Option<String> {
    r.diagnostics
        .iter()
        .find(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
}

fn assert_rejects_e1002(r: &hy_check::CheckResult, what: &str) {
    let codes = error_codes(r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {}, got {:?}",
        what,
        codes
    );
    assert_eq!(r.verdict, Verdict::Reject, "{}", what);
}

// ===========================================================================
// Import-shadowing + named-result guards extracted from PR #535 (filacocta).
// The buildable shadow-aware import-use behavior itself landed on main via
// the #475/#430 buildable work; these regression pins preserve the PR's
// systematic surface family (nested blocks, closures, params, assigned vars,
// scope-end re-use, named results) with full author credit.
// ===========================================================================

#[test]
fn import_shadowed_by_read_local_is_unused() {
    let r = check("import \"strings\"\nfn main() { strings := 1\nfmt.println(strings) }");
    assert_rejects_e1002(&r, "a read of a local strings is not a package use");
    assert!(r
        .diagnostics
        .iter()
        .any(|d| d.code == "HYB-E1002" && d.message.contains("import `strings`")));
}

#[test]
fn import_shadowed_in_nested_block_is_unused() {
    let r =
        check("import \"strings\"\nfn main() { if true { strings := 1\nfmt.println(strings) } }");
    assert_rejects_e1002(&r, "a nested local strings is not a package use");
}

#[test]
fn import_shadowed_by_captured_local_is_unused() {
    let r = check(
        "import \"strings\"\nfn main() { strings := 1\nf := fn() { fmt.println(strings) }\nf() }",
    );
    assert_rejects_e1002(&r, "a captured local strings is not a package use");
}

#[test]
fn import_shadowed_by_parameter_is_unused() {
    let r = check("import \"strings\"\nfn print_number(strings int) { fmt.println(strings) }\nfn main() { print_number(1) }");
    assert_rejects_e1002(&r, "a parameter strings is not a package use");
}

#[test]
fn import_shadowed_by_assigned_var_is_unused() {
    let r = check(
        "import \"strings\"\nfn main() { var strings int\nstrings = 1\nfmt.println(strings) }",
    );
    assert_rejects_e1002(
        &r,
        "a store and read of an exempt local is not a package use",
    );
}

#[test]
fn import_use_after_shadow_scope_ends_accepts() {
    let r = check("import \"strings\"\nfn main() { if true { strings := 1\nfmt.println(strings) }\nfmt.println(strings.repeat(\"a\", 3)) }");
    assert!(error_codes(&r).is_empty(), "{:?}", error_codes(&r));
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn import_use_before_local_binding_accepts() {
    let r = check("import \"strings\"\nfn main() { strings := strings.repeat(\"a\", 3)\nfmt.println(strings) }");
    assert!(error_codes(&r).is_empty(), "{:?}", error_codes(&r));
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn import_shadowed_by_named_result_is_unused() {
    let r = check(
        r#"import "strings"
fn f() (strings int) {
    strings = 1
    return strings
}
fn main() { fmt.println(f()) }
"#,
    );
    assert_rejects_e1002(&r, "import shadowed by named result is unused");
    assert!(r
        .diagnostics
        .iter()
        .any(|d| d.code == "HYB-E1002" && d.message.contains("import `strings`")));
}

#[test]
fn import_shadowed_by_literal_named_result_is_unused() {
    let r = check(
        r#"import "strings"
fn main() {
    f := fn() (strings int) {
        strings = 1
        return strings
    }
    fmt.println(f())
}
"#,
    );
    assert_rejects_e1002(&r, "import shadowed by literal named result is unused");
    assert!(r
        .diagnostics
        .iter()
        .any(|d| d.code == "HYB-E1002" && d.message.contains("import `strings`")));
}

#[test]
fn named_result_signature_import_accepts() {
    let r = check(
        r#"import "time"
fn f() (time time.duration) { return }
fn main() { fmt.println(f()) }
"#,
    );
    assert!(error_codes(&r).is_empty(), "{:?}", error_codes(&r));
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn literal_named_result_signature_import_accepts() {
    let r = check(
        r#"import "time"
fn main() {
    f := fn() (time time.duration) { return }
    fmt.println(f())
}
"#,
    );
    assert!(error_codes(&r).is_empty(), "{:?}", error_codes(&r));
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn import_use_after_named_result_scope_accepts() {
    let r = check(
        r#"import "strings"
fn f() (strings int) {
    strings = 1
    return strings
}
fn main() {
    fmt.println(f())
    fmt.println(strings.repeat("a", 2))
}
"#,
    );
    assert!(error_codes(&r).is_empty(), "{:?}", error_codes(&r));
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn import_use_after_literal_named_result_scope_accepts() {
    let r = check(
        r#"import "strings"
fn main() {
    f := fn() (strings int) {
        strings = 1
        return strings
    }
    fmt.println(f())
    fmt.println(strings.repeat("a", 2))
}
"#,
    );
    assert!(error_codes(&r).is_empty(), "{:?}", error_codes(&r));
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn named_result_redefine_without_fresh_name_is_rejected() {
    let r = check(
        r#"fn f() (n int) {
    n := 1
    return n
}
fn main() { fmt.println(f()) }
"#,
    );
    assert_rejects_e1002(&r, "named result redefine without fresh name is rejected");
    assert!(r
        .diagnostics
        .iter()
        .any(|d| d.code == "HYB-E1002" && d.message.contains("no new variable")));
}

#[test]
fn literal_named_result_redefine_without_fresh_name_is_rejected() {
    let r = check(
        r#"fn main() {
    f := fn() (n int) {
        n := 1
        return n
    }
    fmt.println(f())
}
"#,
    );
    assert_rejects_e1002(
        &r,
        "literal named result redefine without fresh name is rejected",
    );
    assert!(r
        .diagnostics
        .iter()
        .any(|d| d.code == "HYB-E1002" && d.message.contains("no new variable")));
}

#[test]
fn named_result_redefine_with_fresh_name_accepts() {
    let r = check(
        r#"fn f() (n int) {
    n, m := 1, 2
    return n + m
}
fn main() { fmt.println(f()) }
"#,
    );
    assert!(error_codes(&r).is_empty(), "{:?}", error_codes(&r));
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn literal_named_result_redefine_with_fresh_name_accepts() {
    let r = check(
        r#"fn main() {
    f := fn() (n int) {
        n, m := 1, 2
        return n + m
    }
    fmt.println(f())
}
"#,
    );
    assert!(error_codes(&r).is_empty(), "{:?}", error_codes(&r));
    assert_eq!(r.verdict, Verdict::Accept);
}
