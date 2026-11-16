//! Regression tests for #4135 (companion to #4085 / #4218 hex family):
//! The full `strconv` call surface must be arity-checked at `hyb check`
//! time. Before this, every member the member table in foreign.rs blesses
//! escaped with any arity and non-callable package-level values/types
//! (`int_size`, `err_range`, `err_syntax`, `num_error`) could be called as
//! functions — both blow up at `go build` instead (Row 13 BUILDABLE:
//! reject fail-closed, HYB-E1002).
//!
//! Arities mirror the Go std signatures each member lowers to via
//! `to_go_name` (snake -> CamelCase, e.g. `parse_int` -> ParseInt), pinned
//! go1.26.9:
//!   * 1 arg: atoi, itoa, parse_bool, format_bool, can_backquote,
//!     is_graphic, is_print, quote, quote_rune, quote_rune_to_ascii,
//!     quote_rune_to_graphic, quote_to_ascii, quote_to_graphic,
//!     quoted_prefix, unquote
//!   * 2 args: parse_complex, parse_float, format_int, format_uint,
//!     append_bool, append_quote, append_quote_rune,
//!     append_quote_rune_to_ascii, append_quote_rune_to_graphic,
//!     append_quote_to_ascii, append_quote_to_graphic, unquote_char
//!   * 3 args: parse_int, parse_uint, append_int, append_uint
//!   * 4 args: format_complex, format_float
//!   * 5 args: append_float
//!   * int_size / err_range / err_syntax / num_error are package-level
//!     values/types, not functions — calling them must reject.
//! No valid call may be rejected (never over-reject; Go accepts all of
//! these).

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_e1002(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "verdict for {:?}: {:?}", src, r.diagnostics);
}

#[test]
fn strconv_one_arg_members_accept_exact_arity() {
    accepts("import \"strconv\"\nfn main() {\n    n, err := strconv.atoi(\"3\")\n    _ = n\n    _ = err\n}\n");
    accepts("import \"strconv\"\nfn main() {\n    s := strconv.itoa(42)\n    _ = s\n}\n");
    accepts("import \"strconv\"\nfn main() {\n    b, err := strconv.parse_bool(\"true\")\n    _ = b\n    _ = err\n}\n");
    accepts("import \"strconv\"\nfn main() {\n    s := strconv.format_bool(true)\n    _ = s\n}\n");
    accepts("import \"strconv\"\nfn main() {\n    s := strconv.quote(\"hi\")\n    _ = s\n}\n");
    accepts(r#"import "strconv"
fn main() {
    s, err := strconv.unquote("\"hi\"")
    _ = s
    _ = err
}
"#);
}

#[test]
fn strconv_two_arg_members_accept_exact_arity() {
    accepts("import \"strconv\"\nfn main() {\n    f, err := strconv.parse_float(\"3.14\", 64)\n    _ = f\n    _ = err\n}\n");
    accepts("import \"strconv\"\nfn main() {\n    s := strconv.format_int(255, 16)\n    _ = s\n}\n");
    accepts("import \"strconv\"\nfn main() {\n    dst := make([]byte, 8)\n    b := strconv.append_bool(dst, true)\n    _ = b\n}\n");
    accepts("import \"strconv\"\nfn main() {\n    dst := make([]byte, 8)\n    s := \"hi\"\n    b := strconv.append_quote(dst, s)\n    _ = b\n}\n");
}

#[test]
fn strconv_three_arg_members_accept_exact_arity() {
    accepts("import \"strconv\"\nfn main() {\n    n, err := strconv.parse_int(\"ff\", 16, 8)\n    _ = n\n    _ = err\n}\n");
    accepts("import \"strconv\"\nfn main() {\n    n, err := strconv.parse_uint(\"100\", 2, 8)\n    _ = n\n    _ = err\n}\n");
    accepts("import \"strconv\"\nfn main() {\n    dst := make([]byte, 8)\n    b := strconv.append_int(dst, 42, 10)\n    _ = b\n}\n");
    accepts("import \"strconv\"\nfn main() {\n    dst := make([]byte, 8)\n    b := strconv.append_uint(dst, 42, 10)\n    _ = b\n}\n");
}

#[test]
fn strconv_four_and_five_arg_members_accept_exact_arity() {
    accepts("import \"strconv\"\nfn main() {\n    s := strconv.format_float(3.14, \"f\", 2, 64)\n    _ = s\n}\n");
    accepts("import \"strconv\"\nfn main() {\n    c, err := strconv.parse_complex(\"1+2i\", 64)\n    s := strconv.format_complex(c, \"g\", 6, 64)\n    _ = s\n    _ = err\n}\n");
    accepts("import \"strconv\"\nfn main() {\n    dst := make([]byte, 8)\n    b := strconv.append_float(dst, 3.14, \"f\", 2, 64)\n    _ = b\n}\n");
}

#[test]
fn strconv_one_arg_members_reject_wrong_arity() {
    rejects_e1002("import \"strconv\"\nfn main() {\n    n, err := strconv.atoi()\n    _ = n\n    _ = err\n}\n");
    rejects_e1002("import \"strconv\"\nfn main() {\n    n, err := strconv.atoi(\"3\", \"3\")\n    _ = n\n    _ = err\n}\n");
    rejects_e1002("import \"strconv\"\nfn main() {\n    s := strconv.itoa()\n    _ = s\n}\n");
    rejects_e1002("import \"strconv\"\nfn main() {\n    s := strconv.quote()\n    _ = s\n}\n");
    rejects_e1002("import \"strconv\"\nfn main() {\n    s, err := strconv.unquote()\n    _ = s\n    _ = err\n}\n");
}

#[test]
fn strconv_multi_arg_members_reject_wrong_arity() {
    // the owner's premise probes on pre-merge main: these passed hyb check
    // with 0 diagnostics before the fix.
    rejects_e1002("import \"strconv\"\nfn main() {\n    n, err := strconv.parse_int(\"x\", 10)\n    _ = n\n    _ = err\n}\n");
    rejects_e1002("import \"strconv\"\nfn main() {\n    n, err := strconv.parse_int(\"x\", 10, 64, 64)\n    _ = n\n    _ = err\n}\n");
    rejects_e1002("import \"strconv\"\nfn main() {\n    f, err := strconv.parse_float(\"3.14\")\n    _ = f\n    _ = err\n}\n");
    rejects_e1002("import \"strconv\"\nfn main() {\n    s := strconv.format_float(3.14, \"f\", 2)\n    _ = s\n}\n");
    rejects_e1002("import \"strconv\"\nfn main() {\n    s := strconv.format_int(255)\n    _ = s\n}\n");
    rejects_e1002("import \"strconv\"\nfn main() {\n    dst := make([]byte, 8)\n    b := strconv.append_int(dst, 42)\n    _ = b\n}\n");
    rejects_e1002("import \"strconv\"\nfn main() {\n    dst := make([]byte, 8)\n    b := strconv.append_float(dst, 3.14, \"f\", 2)\n    _ = b\n}\n");
}

#[test]
fn strconv_package_values_and_types_are_not_callable() {
    rejects_e1002("import \"strconv\"\nfn main() {\n    n := strconv.int_size(8)\n    _ = n\n}\n");
    rejects_e1002("import \"strconv\"\nfn main() {\n    r, err := strconv.err_range(nil)\n    _ = r\n    _ = err\n}\n");
    rejects_e1002("import \"strconv\"\nfn main() {\n    e := strconv.err_syntax()\n    _ = e\n}\n");
    rejects_e1002("import \"strconv\"\nfn main() {\n    n := strconv.num_error(nil)\n    _ = n\n}\n");
}

#[test]
fn strconv_spread_is_rejected_on_non_variadic_calls() {
    // spread (`...`) on hex members is not yet rejected — same class as
    // #4271, fixed for url/sort/path/filepath (owner note on PR #4238);
    // strconv ships with the guard from day one.
    rejects_e1002("import \"strconv\"\nfn main() {\n    a := [\"3\", \"3\"]\n    n, err := strconv.atoi(a...)\n    _ = n\n    _ = err\n}\n");
    rejects_e1002("import \"strconv\"\nfn main() {\n    a := [\"ff\", 16, 8]\n    n, err := strconv.parse_int(a...)\n    _ = n\n    _ = err\n}\n");
}

#[test]
fn strconv_unrelated_local_calls_still_work() {
    // non-package calls and other packages must be untouched.
    accepts("fn main() {\n    s := \"3\"\n    n := len(s)\n    _ = n\n}\n");
    accepts("import \"strings\"\nfn main() {\n    s := strings.to_upper(\"hi\")\n    _ = s\n}\n");
}

#[test]
fn strconv_arity_error_message_is_actionable() {
    let r = check("import \"strconv\"\nfn main() {\n    n, err := strconv.parse_int(\"x\", 10)\n    _ = n\n    _ = err\n}\n");
    let msg = format!("{:?}", r.diagnostics);
    assert!(msg.contains("takes exactly 3 argument"), "message must state expected arity: {}", msg);
    assert!(msg.contains("got 2"), "message must state actual count: {}", msg);
    assert!(msg.contains("ParseInt"), "message must name the Go lowering: {}", msg);
}
