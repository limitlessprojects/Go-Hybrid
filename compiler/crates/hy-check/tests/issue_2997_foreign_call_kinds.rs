//! issue_2997_foreign_call_kinds.rs — tests for HYB-E1002 rejection of
//! foreign package call results and arguments that violate Go types.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_rejects_e1002(src: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected Reject for snippet:\n{src}\ngot Accept with diagnostics: {:?}",
        r.diagnostics
    );
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for snippet:\n{src}\ngot: {:?}",
        r.diagnostics
    );
}

#[test]
fn test_strings_contains_result_bool_as_int_rejects() {
    let src = r#"
fn main() {
    var n int = strings.contains("ab", "a")
    _ = n
}
"#;
    assert_rejects_e1002(src);
}

#[test]
fn test_strings_index_result_int_as_string_rejects() {
    let src = r#"
fn main() {
    var s string = strings.index("ab", "a")
    _ = s
}
"#;
    assert_rejects_e1002(src);
}

#[test]
fn test_strings_split_result_slice_as_int_rejects() {
    let src = r#"
fn main() {
    var n int = strings.split("a,b", ",")
    _ = n
}
"#;
    assert_rejects_e1002(src);
}

#[test]
fn test_strings_split_element_as_int_rejects() {
    let src = r#"
fn main() {
    parts := strings.split("a,b", ",")
    var n int = parts[0]
    _ = n
}
"#;
    assert_rejects_e1002(src);
}

#[test]
fn test_strings_fields_for_in_element_as_int_rejects() {
    let src = r#"
fn main() {
    for p in strings.fields("a b") {
        var n int = p
        _ = n
    }
}
"#;
    assert_rejects_e1002(src);
}

#[test]
fn test_strings_trim_space_as_bool_rejects() {
    let src = r#"
fn main() {
    var b bool = strings.trim_space(" a ")
    _ = b
}
"#;
    assert_rejects_e1002(src);
}

#[test]
fn test_strings_repeat_wrong_arg_type_rejects() {
    let src = r#"
fn main() {
    _ = strings.repeat("a", "3")
}
"#;
    assert_rejects_e1002(src);
}

#[test]
fn test_strconv_atoi_as_string_rejects() {
    let src = r#"
fn main() {
    n, err := strconv.atoi("3")
    var s string = n
    _ = s
    _ = err
}
"#;
    assert_rejects_e1002(src);
}

#[test]
fn test_strconv_itoa_as_int_rejects() {
    let src = r#"
fn main() {
    var n int = strconv.itoa(3)
    _ = n
}
"#;
    assert_rejects_e1002(src);
}

#[test]
fn test_strconv_itoa_wrong_arg_type_rejects() {
    let src = r#"
fn main() {
    _ = strconv.itoa("3")
}
"#;
    assert_rejects_e1002(src);
}

#[test]
fn test_strconv_parse_float_as_int_rejects() {
    let src = r#"
fn main() {
    f, err := strconv.parse_float("3.5", 64)
    var n int = f
    _ = n
    _ = err
}
"#;
    assert_rejects_e1002(src);
}

#[test]
fn test_fmt_sprintf_as_int_rejects() {
    let src = r#"
fn main() {
    var n int = fmt.sprintf("%d", 3)
    _ = n
}
"#;
    assert_rejects_e1002(src);
}

#[test]
fn test_fmt_sprint_as_int_rejects() {
    let src = r#"
fn main() {
    s := fmt.sprint(3)
    var n int = s
    _ = n
}
"#;
    assert_rejects_e1002(src);
}

#[test]
fn test_os_getenv_as_int_rejects() {
    let src = r#"
fn main() {
    var n int = os.getenv("X")
    _ = n
}
"#;
    assert_rejects_e1002(src);
}

#[test]
fn test_os_args_as_int_rejects() {
    let src = r#"
fn main() {
    var n int = os.args
    _ = n
}
"#;
    assert_rejects_e1002(src);
}

#[test]
fn test_time_now_as_int_rejects() {
    let src = r#"
fn main() {
    var n int = time.now()
    _ = n
}
"#;
    assert_rejects_e1002(src);
}

#[test]
fn test_time_since_as_string_rejects() {
    let src = r#"
fn main() {
    var s string = time.since(time.now())
    _ = s
}
"#;
    assert_rejects_e1002(src);
}

#[test]
fn test_sort_ints_void_used_as_value_rejects() {
    let src = r#"
fn main() {
    var n int = sort.ints([]int{1})
    _ = n
}
"#;
    assert_rejects_e1002(src);
}

#[test]
fn test_sort_ints_wrong_slice_type_rejects() {
    let src = r#"
fn main() {
    sort.ints([]string{"a"})
}
"#;
    assert_rejects_e1002(src);
}

#[test]
fn test_errors_new_as_string_rejects() {
    let src = r#"
fn main() {
    e := errors.new("x")
    var s string = e
    _ = s
}
"#;
    assert_rejects_e1002(src);
}

#[test]
fn test_valid_foreign_package_calls_accepted() {
    let src = r#"
fn main() {
    var b bool = strings.contains("ab", "a")
    var i int = strings.index("ab", "a")
    var parts []string = strings.split("a,b", ",")
    var s string = strings.trim_space(" a ")
    var rep string = strings.repeat("a", 3)
    var itoa string = strconv.itoa(3)
    var fmt_str string = fmt.sprintf("%d", 3)
    var env string = os.getenv("HOME")
    sort.ints([]int{2, 1})
    e := errors.new("err")
    _ = b
    _ = i
    _ = parts
    _ = s
    _ = rep
    _ = itoa
    _ = fmt_str
    _ = env
    _ = e
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept, got: {:?}", r.diagnostics);
}
