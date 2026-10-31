//! issue_2444_data_carrying_enum_equality.rs — SPEC-LANGUAGE §2.5 T16:
//! "==/!= are defined only for unit-only enums (all variants payload-free).
//!  Data-carrying enums compare via match."

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "test_enum_eq.hy";
    let ast = parse_file(path, src).unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_with(src: &str, code: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == code),
        "expected {}, got {:?}\nsrc:\n{}",
        code,
        cs,
        src
    );
    assert_eq!(r.verdict, Verdict::Reject, "src:\n{}", src);
}

fn accepts_without(src: &str, code: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == code),
        "{} must not fire, got {:?}\nsrc:\n{}",
        code,
        cs,
        src
    );
    assert_eq!(r.verdict, Verdict::Accept, "src:\n{}", src);
}

#[test]
fn data_carrying_enum_equality_is_rejected() {
    // 1. Minimal reproducer from issue #2444:
    const REPRO_2444: &str = r#"
enum packet {
    data([]byte)
    ping
}

fn is_same(a packet, b packet) bool {
    return a == b
}

fn main() {
    p1 := packet.data([]byte{1, 2})
    p2 := packet.data([]byte{1, 2})
    _ = is_same(p1, p2)
}
"#;
    rejects_with(REPRO_2444, "HYB-E2006");

    // 2. Inequality (!=) on data-carrying enum:
    const NEQ_ENUM: &str = r#"
enum packet {
    data([]byte)
    ping
}

fn is_diff(a packet, b packet) bool {
    return a != b
}
"#;
    rejects_with(NEQ_ENUM, "HYB-E2006");

    // 3. Direct variable equality without function param:
    const DIRECT_EQ: &str = r#"
enum event {
    click(int)
    quit
}

fn main() {
    e1 := event.click(1)
    e2 := event.quit
    if e1 == e2 {
    }
}
"#;
    rejects_with(DIRECT_EQ, "HYB-E2006");

    // 4. Struct containing a data-carrying enum:
    const STRUCT_WITH_DATA_ENUM: &str = r#"
enum payload {
    body([]byte)
    empty
}

struct wrapper {
    p payload
}

fn test(w1 wrapper, w2 wrapper) bool {
    return w1 == w2
}
"#;
    rejects_with(STRUCT_WITH_DATA_ENUM, "HYB-E2006");

    // 5. Array of data-carrying enums:
    const ARRAY_OF_DATA_ENUM: &str = r#"
enum payload {
    body([]byte)
    empty
}

fn test(a1 [2]payload, a2 [2]payload) bool {
    return a1 == a2
}
"#;
    rejects_with(ARRAY_OF_DATA_ENUM, "HYB-E2006");
}

#[test]
fn unit_only_enum_equality_stays_accepted() {
    // Unit-only enum (all variants payload-free) has valid == and != per SPEC T16
    const UNIT_ONLY: &str = r#"
enum mode {
    read
    write
    exec
}

fn is_same_mode(m1 mode, m2 mode) bool {
    return m1 == m2
}

fn is_diff_mode(m1 mode, m2 mode) bool {
    return m1 != m2
}

struct config {
    m mode
    timeout int
}

fn compare_config(c1 config, c2 config) bool {
    return c1 == c2
}
"#;
    accepts_without(UNIT_ONLY, "HYB-E2006");
}
