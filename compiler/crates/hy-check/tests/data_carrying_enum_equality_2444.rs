//! data_carrying_enum_equality_2444.rs — regression for issue #2444:
//! [soundness] Data-carrying enum equality == / != bypasses SPEC T16 & HYB-E2006,
//! failing go build (cannot be compared).
//!
//! SPEC-LANGUAGE §2.5 T16 explicitly mandates:
//! "==/!= are defined only for unit-only enums (all variants payload-free).
//!  Data-carrying enums compare via match.
//!  Why: payload equality is deep-equality policy (reference vs structural)
//!  that MVP does not need and must not guess."
//!
//! Any equality comparison (== or !=) on data-carrying enums or structs/arrays
//! containing them must be rejected at check time with HYB-E2006.

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

fn error_messages(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.message.clone())
        .collect()
}

// ---------------------------------------------------------------------------
// Regressions: Data-carrying enums must reject equality with HYB-E2006
// ---------------------------------------------------------------------------

#[test]
fn shape_01_data_carrying_enum_equality_in_fn_args_rejects_e2006() {
    let src = r#"
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
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected rejection for data-carrying enum ==");
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E2006"),
        "expected HYB-E2006 in error codes: {:?}",
        error_codes(&r)
    );
    assert!(
        error_messages(&r).iter().any(|m| m.contains("data-carrying enum") || m.contains("HYB-E2006")),
        "expected error message to explain data-carrying enum incompatibility: {:?}",
        error_messages(&r)
    );
}

#[test]
fn shape_02_data_carrying_enum_inequality_in_fn_args_rejects_e2006() {
    let src = r#"
enum packet {
    data([]byte)
    ping
}

fn is_diff(a packet, b packet) bool {
    return a != b
}

fn main() {
    p1 := packet.data([]byte{1, 2})
    p2 := packet.ping
    _ = is_diff(p1, p2)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected rejection for data-carrying enum !=");
    assert!(
        error_codes(&r).iter().any(|c| c == "HYB-E2006"),
        "expected HYB-E2006 in error codes: {:?}",
        error_codes(&r)
    );
}

#[test]
fn shape_03_data_carrying_enum_local_vars_rejects_e2006() {
    let src = r#"
enum event {
    click(int)
    idle
}

fn main() {
    e1 := event.click(1)
    e2 := event.click(2)
    same := e1 == e2
    fmt.println(same)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).iter().any(|c| c == "HYB-E2006"));
}

#[test]
fn shape_04_data_carrying_enum_direct_variant_calls_rejects_e2006() {
    let src = r#"
enum message {
    text(string)
    quit
}

fn main() {
    _ = message.text("hello") == message.quit
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).iter().any(|c| c == "HYB-E2006"));
}

#[test]
fn shape_05_struct_containing_data_carrying_enum_rejects_e2006() {
    let src = r#"
enum payload {
    body([]byte)
    empty
}

struct envelope {
    p payload
    id int
}

fn main() {
    var e1 envelope
    var e2 envelope
    _ = e1 == e2
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "struct with data-carrying enum must reject ==");
    assert!(error_codes(&r).iter().any(|c| c == "HYB-E2006"));
}

#[test]
fn shape_06_type_aliased_data_carrying_enum_rejects_e2006() {
    let src = r#"
enum packet {
    data([]byte)
    ping
}

type packet_alias = packet

fn check_alias(a packet_alias, b packet_alias) bool {
    return a == b
}

fn main() {
    var p1 packet_alias
    var p2 packet_alias
    _ = check_alias(p1, p2)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).iter().any(|c| c == "HYB-E2006"));
}

// ---------------------------------------------------------------------------
// Positive Controls: Unit-only enums must STAY accepted per SPEC T16
// ---------------------------------------------------------------------------

#[test]
fn positive_control_01_unit_only_enum_stays_accepted() {
    let src = r#"
enum mode {
    read
    write
    append
}

fn is_read(m mode) bool {
    return m == mode.read
}

fn main() {
    m1 := mode.read
    m2 := mode.write
    fmt.println(m1 == m2, is_read(m1))
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "unit-only enums must accept == (SPEC T16): {:?}", r.diagnostics);
}

#[test]
fn positive_control_02_unit_only_enum_inequality_stays_accepted() {
    let src = r#"
enum state {
    stopped
    running
}

fn check_state(a state, b state) bool {
    return a != b
}

fn main() {
    fmt.println(check_state(state.stopped, state.running))
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "unit-only enums must accept != (SPEC T16): {:?}", r.diagnostics);
}

#[test]
fn positive_control_03_struct_containing_unit_only_enum_stays_accepted() {
    let src = r#"
enum color {
    red
    green
    blue
}

struct pixel {
    c color
    alpha int
}

fn main() {
    p1 := pixel{c: color.red, alpha: 255}
    p2 := pixel{c: color.blue, alpha: 255}
    fmt.println(p1 == p2)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "struct with unit-only enum must stay accepted: {:?}", r.diagnostics);
}
