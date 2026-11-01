//! foreign_call_kinds_2997.rs: regression for #2997.
//!
//! Curated package calls (`strings`, `strconv`, `fmt.sprint*`, `os.getenv`)
//! had `OpKind::Unknown` results, so a `bool` result stored into an `int`
//! (and similar) passed hyb check and failed go build. Correct uses, and
//! calls not in the result table, must keep accepting.

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

fn rejects(body: &str) {
    let src = format!("fn main() {{\n{}\n}}\n", body);
    let r = check(&src);
    let cs = error_codes(&r);
    assert!(cs.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002 for {:?}, got {:?}", src, cs);
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts(body: &str) {
    let src = format!("fn main() {{\n{}\n}}\n", body);
    let r = check(&src);
    let cs = error_codes(&r);
    assert!(cs.is_empty(), "must accept {:?}, got {:?}", src, cs);
}

#[test]
fn strings_bool_result_into_int_rejects() {
    rejects("    var n int = strings.contains(\"ab\", \"a\")\n    fmt.println(n)");
}

#[test]
fn strings_int_result_into_string_rejects() {
    rejects("    var s string = strings.index(\"ab\", \"a\")\n    fmt.println(s)");
}

#[test]
fn strings_split_result_into_int_rejects() {
    rejects("    var n int = strings.split(\"a,b\", \",\")\n    fmt.println(n)");
}

#[test]
fn strings_split_element_into_int_rejects() {
    rejects("    parts := strings.split(\"a,b\", \",\")\n    var n int = parts[0]\n    fmt.println(n)");
}

#[test]
fn strings_fields_loop_var_into_int_rejects() {
    rejects("    for p in strings.fields(\"a b\") {\n        var n int = p\n        fmt.println(n)\n    }");
}

#[test]
fn strings_trim_space_into_bool_rejects() {
    rejects("    var b bool = strings.trim_space(\" a \")\n    fmt.println(b)");
}

#[test]
fn strconv_itoa_into_int_rejects() {
    rejects("    var n int = strconv.itoa(3)\n    fmt.println(n)");
}

#[test]
fn strconv_atoi_value_into_string_rejects() {
    rejects("    n, err := strconv.atoi(\"3\")\n    var s string = n\n    fmt.println(s, err)");
}

#[test]
fn strconv_parse_float_into_int_rejects() {
    rejects("    f, err := strconv.parse_float(\"3.5\", 64)\n    var n int = f\n    fmt.println(n, err)");
}

#[test]
fn fmt_sprintf_into_int_rejects() {
    rejects("    var n int = fmt.sprintf(\"%d\", 3)\n    fmt.println(n)");
}

#[test]
fn fmt_sprint_local_into_int_rejects() {
    rejects("    s := fmt.sprint(3)\n    var n int = s\n    fmt.println(n)");
}

#[test]
fn os_getenv_into_int_rejects() {
    rejects("    var n int = os.getenv(\"X\")\n    fmt.println(n)");
}

#[test]
fn correct_uses_accept() {
    accepts("    var b bool = strings.contains(\"ab\", \"a\")\n    var i int = strings.index(\"ab\", \"a\")\n    var s string = strings.trim_space(\" a \")\n    parts := strings.split(\"a,b\", \",\")\n    var p string = parts[0]\n    var t string = strconv.itoa(3)\n    n, err := strconv.atoi(\"3\")\n    var m int = n\n    f, err2 := strconv.parse_float(\"3.5\", 64)\n    var g float64 = f\n    var u string = fmt.sprintf(\"%d\", 3)\n    var e string = os.getenv(\"X\")\n    fmt.println(b, i, s, p, t, m, err, g, err2, u, e)");
}

#[test]
fn string_concat_and_compare_with_results_accept() {
    accepts("    s := strings.to_upper(\"a\") + strconv.itoa(1)\n    ok := strings.index(s, \"A\") >= 0 && strings.has_prefix(s, \"A\")\n    fmt.println(s, ok)");
}

#[test]
fn shadowed_package_name_stays_local() {
    accepts("    strings := 3\n    var n int = strings\n    fmt.println(n)");
}
