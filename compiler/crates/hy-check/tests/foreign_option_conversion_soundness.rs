//! Regression tests for foreign named type and option conversion soundness (Row 13 BUILDABLE):
//! Reject invalid type conversions to `option[T]` and between foreign handles and scalars.
//!
//! Under Go specification (§Conversions) and Row 13 of GUARANTEES.md:
//! - In Go, `option[T]` is a struct `hyOption[T]`, and foreign types (`os.File`, `net.Conn`,
//!   `sync.Mutex`) are named Go types.
//! - Go forbids converting non-structs/scalars to struct types or vice versa.
//! - `hy-check` must reject `int_opt(42)` (where `type int_opt = option[int]`), `int(f)` (where f is `os.file`),
//!   and `my_file(42)` (where `type my_file = os.file`) with HYB-E1002, while permitting conversions to `any` (`any(f)`).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("conversion_soundness_test.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn count_e1002(r: &hy_check::CheckResult) -> usize {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error && d.code == "HYB-E1002")
        .count()
}

#[test]
fn non_option_to_option_alias_conversion_is_rejected() {
    let cases = [
        "type int_opt = option[int]\nfn main() {\n    x := 42\n    o := int_opt(x)\n    _ = o\n}\n",
        "type str_opt = option[string]\nfn main() {\n    s := \"abc\"\n    o := str_opt(s)\n    _ = o\n}\n",
        "type bool_opt = option[bool]\nfn main() {\n    b := true\n    o := bool_opt(b)\n    _ = o\n}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected reject for conversion to option alias: {src}"
        );
        assert!(
            count_e1002(&r) >= 1,
            "expected HYB-E1002 for conversion to option alias: {src}, got {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn foreign_handle_to_scalar_conversion_is_rejected() {
    let cases = [
        "fn main() {\n    var f os.file\n    n := int(f)\n    _ = n\n}\n",
        "fn main() {\n    var f os.file\n    s := string(f)\n    _ = s\n}\n",
        "fn main() {\n    var mu sync.mutex\n    b := bool(mu)\n    _ = b\n}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected reject for foreign to scalar conversion: {src}"
        );
        assert!(
            count_e1002(&r) >= 1,
            "expected HYB-E1002 for foreign to scalar conversion: {src}, got {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn scalar_to_foreign_handle_alias_conversion_is_rejected() {
    let cases = [
        "type my_file = os.file\nfn main() {\n    n := 42\n    f := my_file(n)\n    _ = f\n}\n",
        "type my_file = os.file\nfn main() {\n    s := \"test\"\n    f := my_file(s)\n    _ = f\n}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected reject for scalar to foreign conversion: {src}"
        );
        assert!(
            count_e1002(&r) >= 1,
            "expected HYB-E1002 for scalar to foreign conversion: {src}, got {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn foreign_to_interface_conversion_accepts() {
    let src = "fn main() {\n    var f os.file\n    a := any(f)\n    _ = a\n}\n";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept for foreign to any conversion: {:?}",
        r.diagnostics
    );
}

#[test]
fn option_to_interface_conversion_accepts() {
    let src = "type int_opt = option[int]\nfn main() {\n    var o int_opt\n    a := any(o)\n    _ = a\n}\n";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept for option to any conversion: {:?}",
        r.diagnostics
    );
}
