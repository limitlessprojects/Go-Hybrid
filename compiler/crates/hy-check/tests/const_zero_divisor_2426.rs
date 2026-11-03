//! Issue #2426 regression test suite:
//! Constant zero divisor check misses int(0.0), len(empty_concat), len/cap([0]T),
//! real/imag(complex), and int_var / 0.0 (HYB-E6001).

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("parses");
    check_file(&ast)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn assert_rejection(src: &str) {
    let cs = codes(src);
    assert!(
        cs.contains(&"HYB-E6001".to_string()),
        "expected HYB-E6001 in {cs:?} for source:\n{src}"
    );
}

fn assert_accepted(src: &str) {
    let cs = codes(src);
    assert!(
        !cs.contains(&"HYB-E6001".to_string()),
        "expected acceptance (no HYB-E6001), got {cs:?} for source:\n{src}"
    );
}

#[test]
fn test_01_int_conv_float_zero_rejects() {
    assert_rejection(
        r#"
fn main() {
    x := 10
    _ = x / int(0.0)
}
"#,
    );
    assert_rejection(
        r#"
fn main() {
    x := 10
    _ = x / uint8(-0.0)
}
"#,
    );
    assert_rejection(
        r#"
fn main() {
    x := 10
    _ = x / i64(1.5 - 1.5)
}
"#,
    );
    assert_rejection(
        r#"
const fz = 0.0

fn main() {
    x := 10
    _ = x / int(fz)
}
"#,
    );
}

#[test]
fn test_02_aliased_int_zero_rejects() {
    assert_rejection(
        r#"
type my_int = int

fn main() {
    x := 10
    _ = x / my_int(0)
}
"#,
    );
    assert_rejection(
        r#"
type my_int = int

fn main() {
    x := 10
    _ = x / my_int(0.0)
}
"#,
    );
}

#[test]
fn test_03_len_empty_string_concat_rejects() {
    assert_rejection(
        r#"
fn main() {
    x := 10
    _ = x / len("" + "")
}
"#,
    );
    assert_rejection(
        r#"
const e = ""

fn main() {
    x := 10
    _ = x / len(e + "")
}
"#,
    );
    assert_rejection(
        r#"
fn main() {
    x := 10
    _ = x / len(("" + "") + "")
}
"#,
    );
}

#[test]
fn test_04_len_cap_zero_sized_array_rejects() {
    assert_rejection(
        r#"
fn main() {
    x := 10
    _ = x / len([0]int{})
}
"#,
    );
    assert_rejection(
        r#"
fn main() {
    x := 10
    _ = x / cap([0]int{})
}
"#,
    );
    assert_rejection(
        r#"
fn main() {
    var a [0]int
    x := 10
    _ = x / len(a)
}
"#,
    );
    assert_rejection(
        r#"
fn main() {
    var a *[0]int
    x := 10
    _ = x / cap(a)
}
"#,
    );
}

#[test]
fn test_05_real_imag_complex_zero_rejects() {
    assert_rejection(
        r#"
fn main() {
    x := 10
    _ = x / int(real(complex(0, 5)))
}
"#,
    );
    assert_rejection(
        r#"
fn main() {
    x := 10
    _ = x / int(imag(complex(5, 0)))
}
"#,
    );
    assert_rejection(
        r#"
fn main() {
    x := 10
    _ = x / real(complex(0.0, 5.0))
}
"#,
    );
}

#[test]
fn test_06_untyped_float_dividing_int_var_rejects() {
    assert_rejection(
        r#"
fn main() {
    x := 10
    _ = x / 0.0
}
"#,
    );
    assert_rejection(
        r#"
fn main() {
    x := 10
    x /= 0.0
}
"#,
    );
    assert_rejection(
        r#"
const fz = 0.0

fn main() {
    x := 10
    _ = x / fz
}
"#,
    );
}

#[test]
fn test_07_minimal_reproducer_from_issue_2426() {
    let src = r#"
fn main() {
    x := 10
    _ = x / int(0.0)
    _ = x / len("" + "")
    _ = x / len([0]int{})
    _ = x / cap([0]int{})
    _ = x / int(real(complex(0, 5)))
    _ = x / 0.0
}
"#;
    let cs = codes(src);
    let count = cs.iter().filter(|c| *c == "HYB-E6001").count();
    assert_eq!(
        count, 6,
        "expected 6 HYB-E6001 diagnostics for minimal reproducer, got {cs:?}"
    );
}

#[test]
fn test_08_positive_controls_stay_accepted() {
    // 1. Runtime float division by float zero (valid Go IEEE-754)
    assert_accepted(
        r#"
fn main() {
    var f float64 = 1.0
    _ = f / 0.0
}
"#,
    );

    // 2. Non-zero fixed array length
    assert_accepted(
        r#"
fn main() {
    x := 10
    _ = x / len([2]int{})
}
"#,
    );

    // 3. Array expression containing function call (runtime evaluation)
    assert_accepted(
        r#"
fn get_zero_arr() [0]int {
    return [0]int{}
}

fn main() {
    x := 10
    _ = x / len(get_zero_arr())
}
"#,
    );
}
