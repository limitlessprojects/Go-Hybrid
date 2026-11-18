//! Regression tests for encoding/binary call arity and symbol validation:
//! The full `encoding/binary` call surface must be arity-checked at `hyb check`
//! time. Before this, calling binary package members with wrong arity or calling
//! non-callable package-level values/types/constants (`big_endian`, `little_endian`,
//! `native_endian`, `max_varint_len16`, `max_varint_len32`, `max_varint_len64`,
//! `byte_order`, `append_byte_order`) passed `hyb check` with any arity and blew up
//! at `go build` instead (Row 13 BUILDABLE: reject fail-closed, HYB-E1002).
//!
//! Arities mirror the Go std signatures each member lowers to via `to_go_name`:
//!   * 1 arg: size, varint, uvarint, read_varint, read_uvarint
//!   * 2 args: put_varint, put_uvarint
//!   * 3 args: read, write, append, encode, decode
//!   * big_endian / little_endian / native_endian / max_varint_len* / byte_order
//!     are package-level values, constants, and types — calling them must reject.
//!   * None of binary's functions accept variadic spread (`...`).

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
fn binary_one_arg_members_accept_exact_arity() {
    accepts(r#"import "encoding/binary"
fn main() {
    n := binary.size(42)
    _ = n
}
"#);
    accepts(r#"import "encoding/binary"
fn main() {
    buf := make([]byte, 10)
    v, n := binary.varint(buf)
    _ = v
    _ = n
}
"#);
    accepts(r#"import "encoding/binary"
fn main() {
    buf := make([]byte, 10)
    u, n := binary.uvarint(buf)
    _ = u
    _ = n
}
"#);
}

#[test]
fn binary_two_arg_members_accept_exact_arity() {
    accepts(r#"import "encoding/binary"
fn main() {
    buf := make([]byte, 10)
    n := binary.put_varint(buf, 42)
    _ = n
}
"#);
    accepts(r#"import "encoding/binary"
fn main() {
    buf := make([]byte, 10)
    n := binary.put_uvarint(buf, 42)
    _ = n
}
"#);
}

#[test]
fn binary_three_arg_members_accept_exact_arity() {
    accepts(r#"import "encoding/binary"
fn main() {
    buf := make([]byte, 8)
    err := binary.read(buf, binary.little_endian, 42)
    _ = err
}
"#);
    accepts(r#"import "encoding/binary"
fn main() {
    buf := make([]byte, 8)
    err := binary.write(buf, binary.big_endian, 42)
    _ = err
}
"#);
}

#[test]
fn binary_one_arg_members_reject_wrong_arity() {
    rejects_e1002(r#"import "encoding/binary"
fn main() {
    n := binary.size()
    _ = n
}
"#);
    rejects_e1002(r#"import "encoding/binary"
fn main() {
    n := binary.size(1, 2)
    _ = n
}
"#);
    rejects_e1002(r#"import "encoding/binary"
fn main() {
    v, n := binary.varint()
    _ = v
    _ = n
}
"#);
    rejects_e1002(r#"import "encoding/binary"
fn main() {
    buf := make([]byte, 8)
    v, n := binary.varint(buf, 10)
    _ = v
    _ = n
}
"#);
}

#[test]
fn binary_multi_arg_members_reject_wrong_arity() {
    // Missing arguments
    rejects_e1002(r#"import "encoding/binary"
fn main() {
    buf := make([]byte, 10)
    n := binary.put_varint(buf)
    _ = n
}
"#);
    rejects_e1002(r#"import "encoding/binary"
fn main() {
    buf := make([]byte, 8)
    err := binary.read(buf)
    _ = err
}
"#);
    rejects_e1002(r#"import "encoding/binary"
fn main() {
    buf := make([]byte, 8)
    err := binary.read(buf, binary.little_endian)
    _ = err
}
"#);
    // Too many arguments
    rejects_e1002(r#"import "encoding/binary"
fn main() {
    buf := make([]byte, 10)
    n := binary.put_varint(buf, 42, 100)
    _ = n
}
"#);
    rejects_e1002(r#"import "encoding/binary"
fn main() {
    buf := make([]byte, 8)
    err := binary.read(buf, binary.little_endian, 42, 99)
    _ = err
}
"#);
}

#[test]
fn binary_package_values_and_types_are_not_callable() {
    rejects_e1002(r#"import "encoding/binary"
fn main() {
    b := binary.big_endian()
    _ = b
}
"#);
    rejects_e1002(r#"import "encoding/binary"
fn main() {
    l := binary.little_endian()
    _ = l
}
"#);
    rejects_e1002(r#"import "encoding/binary"
fn main() {
    n := binary.native_endian()
    _ = n
}
"#);
    rejects_e1002(r#"import "encoding/binary"
fn main() {
    m := binary.max_varint_len64()
    _ = m
}
"#);
    rejects_e1002(r#"import "encoding/binary"
fn main() {
    m := binary.max_varint_len32()
    _ = m
}
"#);
    rejects_e1002(r#"import "encoding/binary"
fn main() {
    m := binary.max_varint_len16()
    _ = m
}
"#);
    rejects_e1002(r#"import "encoding/binary"
fn main() {
    t := binary.byte_order()
    _ = t
}
"#);
}

#[test]
fn binary_spread_is_rejected_on_non_variadic_calls() {
    rejects_e1002(r#"import "encoding/binary"
fn main() {
    a := [42]
    n := binary.size(a...)
    _ = n
}
"#);
    rejects_e1002(r#"import "encoding/binary"
fn main() {
    buf := make([]byte, 10)
    n := binary.put_varint(buf, 42...)
    _ = n
}
"#);
}

#[test]
fn binary_arity_error_message_is_actionable() {
    let r = check(r#"import "encoding/binary"
fn main() {
    n := binary.size()
    _ = n
}
"#);
    assert_eq!(r.verdict, Verdict::Reject);
    let diag = r.diagnostics.iter().find(|d| d.code == "HYB-E1002").unwrap();
    assert!(
        diag.message.contains("`binary.size` takes exactly 1 argument, got 0"),
        "unexpected message: {}",
        diag.message
    );
    assert!(
        diag.notes.iter().any(|n| n.contains("`binary.Size`")),
        "notes must cite Go name: {:?}",
        diag.notes
    );
}
