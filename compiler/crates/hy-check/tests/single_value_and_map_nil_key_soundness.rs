//! Regression tests for Soundness violations under Row 13 of GUARANTEES.md (rule BUILDABLE):
//! - #1850: Unary operators (! and -) omit single-value context checks for multi-value calls
//! - #1849: Composite slice ([]T{...}) and map (map[K]V{...}) literals omit single-value context checks for multi-value calls
//! - #1848: Map composite literals omit nil key checks for non-nilable key types

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let f = parse_file("test.hy", src).expect("parse ok");
    check_file(&f)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn test_issue_1850_unary_multi_value_call_rejected() {
    let not_src = r#"
fn two_bools() (bool, bool) {
    return true, false
}

fn main() {
    var b = !two_bools()
    println(b)
}
"#;
    let c = codes(not_src);
    assert!(c.iter().any(|code| code == "HYB-E1002"), "expected HYB-E1002 for !two_bools(), got {:?}", c);

    let neg_src = r#"
fn two_ints() (int, int) {
    return 1, 2
}

fn main() {
    var x = -two_ints()
    println(x)
}
"#;
    let c2 = codes(neg_src);
    assert!(c2.iter().any(|code| code == "HYB-E1002"), "expected HYB-E1002 for -two_ints(), got {:?}", c2);
}

#[test]
fn test_issue_1849_slice_and_map_literal_multi_value_rejected() {
    let slice_src = r#"
fn two_ints() (int, int) {
    return 10, 20
}

fn main() {
    var s = []int{ two_ints() }
    println(s[0])
}
"#;
    let c = codes(slice_src);
    assert!(c.iter().any(|code| code == "HYB-E1002"), "expected HYB-E1002 for []int{ two_ints() }, got {:?}", c);

    let map_val_src = r#"
fn two_ints() (int, int) {
    return 10, 20
}

fn main() {
    var m = map[string]int{ "val": two_ints() }
    println(m["val"])
}
"#;
    let c2 = codes(map_val_src);
    assert!(c2.iter().any(|code| code == "HYB-E1002"), "expected HYB-E1002 for map[string]int{ \"val\": two_ints() }, got {:?}", c2);

    let map_key_src = r#"
fn two_strings() (string, string) {
    return "a", "b"
}

fn main() {
    var m = map[string]int{ two_strings(): 42 }
    println(m["a"])
}
"#;
    let c3 = codes(map_key_src);
    assert!(c3.iter().any(|code| code == "HYB-E1002"), "expected HYB-E1002 for map key multi-value call, got {:?}", c3);
}

#[test]
fn test_issue_1848_map_nil_key_rejected_for_non_nilable() {
    let map_int_key = r#"
fn main() {
    var m = map[int]string{ nil: "alpha" }
    println(m[0])
}
"#;
    let c = codes(map_int_key);
    assert!(
        c.iter().any(|code| code == "HYB-E1002" || code == "HYB-E2017" || code == "HYB-E2019"),
        "expected error for map[int]string with nil key, got {:?}",
        c
    );

    let map_str_key = r#"
fn main() {
    var m = map[string]int{ nil: 42 }
    println(m["a"])
}
"#;
    let c2 = codes(map_str_key);
    assert!(
        c2.iter().any(|code| code == "HYB-E1002" || code == "HYB-E2017" || code == "HYB-E2019"),
        "expected error for map[string]int with nil key, got {:?}",
        c2
    );
}

#[test]
fn test_valid_unary_and_literals_still_pass() {
    let valid_src = r#"
fn one_bool() bool {
    return true
}

fn one_int() int {
    return 42
}

fn main() {
    var b = !one_bool()
    var x = -one_int()
    var s = []int{ one_int(), 100 }
    var m = map[string]int{ "key": one_int() }
    println(b, x, s[0], m["key"])
}
"#;
    let c = codes(valid_src);
    assert!(c.is_empty(), "expected clean pass for valid single-value calls, got {:?}", c);
}
