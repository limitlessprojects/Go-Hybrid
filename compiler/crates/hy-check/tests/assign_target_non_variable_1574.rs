//! assign_target_non_variable_1574.rs — an assignment target that is not a
//! variable (issue #1574).
//!
//! `place()` → `PlaceKind::Ident` only looked the name up in the lexical
//! scopes and otherwise called `mention()`, so a store into a package-scope
//! FUNCTION declaration, an imported PACKAGE, a predeclared BUILTIN or a TYPE
//! name passed `hyb check` with 0 diagnostics and then failed `go build`
//! (GUARANTEES row 13, BUILDABLE):
//!
//!   f = 2      -> cannot assign to f (neither addressable nor a map index expression)
//!   f++        -> invalid operation: f++ (non-numeric type func() int)
//!   fmt = 1    -> use of package fmt not in selector
//!   len = 1    -> len (built-in) must be called
//!   holder = 1 -> holder (type) is not an expression
//!
//! The value-position leg of the same class was #778 (closed); this is the
//! assignment-target leg, and `check_type_name_in_value_position` is only
//! reached from `Expr::Ident`, never from a `Place`.
//!
//! The check must stay fail-open when a binding shadows the package name
//! (`len := 5; len = 6` is a legal local write) and for `_`.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_rejects(src: &str, what: &str) {
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {what}, got {cs:?}"
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {what}");
}

fn assert_accepts(src: &str, what: &str) {
    let r = check(src);
    let cs = codes(&r);
    assert!(cs.is_empty(), "expected ACCEPT for {what}, got {cs:?}");
    assert_eq!(r.verdict, Verdict::Accept, "verdict for {what}");
}

// ---------------------------------------------------------------------------
// Reject shapes.
// ---------------------------------------------------------------------------

#[test]
fn assign_to_fn_name_rejects() {
    assert_rejects(
        "fn f() int {\n    return 1\n}\n\nfn main() {\n    f = 2\n    fmt.println(f)\n}\n",
        "`f = 2` where f is a fn declaration",
    );
}

#[test]
fn inc_fn_name_rejects() {
    assert_rejects(
        "fn f() int {\n    return 1\n}\n\nfn main() {\n    f++\n}\n",
        "`f++` where f is a fn declaration",
    );
}

#[test]
fn compound_assign_fn_name_rejects() {
    assert_rejects(
        "fn f() int {\n    return 1\n}\n\nfn main() {\n    f += 1\n    fmt.println(\"x\")\n}\n",
        "`f += 1` where f is a fn declaration",
    );
}

#[test]
fn multi_target_assign_fn_name_rejects() {
    assert_rejects(
        "fn f() int {\n    return 1\n}\n\nfn main() {\n    var x int\n    f, x = 1, 2\n    fmt.println(x)\n}\n",
        "`f, x = 1, 2` where f is a fn declaration",
    );
}

#[test]
fn assign_to_package_name_rejects() {
    assert_rejects(
        "fn main() {\n    fmt = 1\n    fmt.println(\"x\")\n}\n",
        "`fmt = 1` where fmt is a package prefix",
    );
}

#[test]
fn assign_to_builtin_rejects() {
    assert_rejects(
        "fn main() {\n    len = 1\n    fmt.println(\"x\")\n}\n",
        "`len = 1` where len is a predeclared builtin",
    );
}

#[test]
fn assign_to_struct_type_rejects() {
    assert_rejects(
        "struct holder {\n    a int\n}\n\nfn main() {\n    holder = 1\n    fmt.println(\"x\")\n}\n",
        "`holder = 1` where holder is a struct type",
    );
}

#[test]
fn assign_to_predeclared_type_rejects() {
    assert_rejects(
        "fn main() {\n    error = 1\n    fmt.println(\"x\")\n}\n",
        "`error = 1` where error is a predeclared type",
    );
}

// ---------------------------------------------------------------------------
// Controls — legal write targets and shadowing.
// ---------------------------------------------------------------------------

#[test]
fn local_var_param_and_blank_targets_accept() {
    let src = "fn g(x int) int {\n    x = 2\n    x++\n    return x\n}\n\nfn main() {\n    var y int\n    y = 3\n    y += 1\n    _ = 1\n    fmt.println(g(y))\n}\n";
    assert_accepts(src, "local var / parameter / blank writes");
}

#[test]
fn shadowing_a_builtin_accepts() {
    // A local binding shadows the predeclared name: this is a legal Go
    // program and must not be rejected by the new target rule.
    assert_accepts(
        "fn main() {\n    len := 5\n    len = 6\n    len++\n    fmt.println(len)\n}\n",
        "`len := 5; len = 6; len++` (local shadow of a builtin)",
    );
}

#[test]
fn function_value_read_accepts() {
    // Only a STORE is illegal; a fn name in value position is a function
    // value.
    assert_accepts(
        "fn f() int {\n    return 1\n}\n\nfn main() {\n    v := f\n    fmt.println(v())\n}\n",
        "`v := f` (function value)",
    );
}

#[test]
fn file_const_stays_with_the_const_rule() {
    // The const-write rule (E3006) owns this shape with a more precise
    // message; the new target rule must not shadow it.
    let r = check("const limit = 10\n\nfn main() {\n    limit = 20\n    fmt.println(limit)\n}\n");
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for a const write, got {cs:?}"
    );
}
