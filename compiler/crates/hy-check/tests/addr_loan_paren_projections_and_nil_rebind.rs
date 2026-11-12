use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn map_slot_loan_parenthesized_projections_and_nil_rebind_rejected_e3104() {
    let invalid_cases = [
        r#"
struct item {
    val int
}
fn main() {
    m := map[string]item{"a": item{val: 1}}
    n := &m["a"]
    p := &(n.val)
    _ = p
}
"#,
        r#"
struct item {
    val int
}
fn main() {
    m := map[string]item{"a": item{val: 1}}
    n := &m["a"]
    p := &(n).val
    _ = p
}
"#,
        r#"
struct item {
    val int
}
fn main() {
    m := map[string]item{"a": item{val: 1}}
    n := &m["a"]
    p := &move n.val
    _ = p
}
"#,
        r#"
struct item {
    val int
}
fn main() {
    m := map[string]item{"a": item{val: 1}}
    n := &m["a"]
    x := move (n.val)
    _ = x
}
"#,
        r#"
struct item {
    val int
}
fn main() {
    m := map[string]item{"a": item{val: 1}}
    n := &m["a"]
    x := move (n).val
    _ = x
}
"#,
        r#"
struct item {
    val int
}
fn main() {
    m := map[string]item{"a": item{val: 1}}
    n := &m["a"]
    n = (nil)
}
"#,
    ];

    for (idx, src) in invalid_cases.iter().enumerate() {
        let parsed = parse_file("repro_addr_loan_paren.hy", src).expect("valid syntax");
        let checked = check_file(&parsed);
        assert_eq!(checked.verdict, Verdict::Reject, "case #{idx} should reject");
        assert!(
            checked.diagnostics.iter().any(|d| d.code == "HYB-E3104"),
            "case #{idx} expected HYB-E3104 diagnostic, got: {:?}",
            checked.diagnostics
        );
    }

    let valid_control = r#"
struct item {
    val int
}
fn main() {
    m := map[string]item{"a": item{val: 1}}
    n := &m["a"]
    n.val = 42
    v := (n.val) + (n).val
    _ = v
}
"#;
    let parsed_ok = parse_file("control_addr_loan_ok.hy", valid_control).expect("valid control syntax");
    let checked_ok = check_file(&parsed_ok);
    assert_eq!(checked_ok.verdict, Verdict::Accept);
    assert!(
        checked_ok.diagnostics.is_empty(),
        "expected 0 diagnostics for valid map-slot loan field access, got: {:?}",
        checked_ok.diagnostics
    );
}
