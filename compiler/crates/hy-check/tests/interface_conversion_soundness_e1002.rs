//! interface_conversion_soundness_e1002.rs — tests for HYB-E1002 rejection of
//! invalid type conversions from interface (any) to concrete types.
//! Issue #2482: SPEC-LANGUAGE §3.1 & GUARANTEES.md Row 13 (BUILDABLE).

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

#[test]
fn interface_conversion_to_scalar_rejects_e1002() {
    let cases = [
        ("any to int", "fn main() {\n    var a any = 1\n    _ = int(a)\n}\n"),
        ("any to float64", "fn main() {\n    var a any = 1.5\n    _ = float64(a)\n}\n"),
        ("any to bool", "fn main() {\n    var a any = true\n    _ = bool(a)\n}\n"),
        ("any to string", "fn main() {\n    var a any = \"hello\"\n    _ = string(a)\n}\n"),
    ];
    for (label, src) in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected reject for {label}");
        assert!(error_codes(&r).contains(&"HYB-E1002".to_string()), "expected HYB-E1002 for {label}, got: {:?}", r.diagnostics);
    }
}

#[test]
fn interface_conversion_to_composite_and_pointer_rejects_e1002() {
    let cases = [
        ("any to pointer", "type p_int = *int\nfn main() {\n    var a any = 1\n    _ = p_int(a)\n}\n"),
        ("any to slice", "type my_slice = []int\nfn main() {\n    var a any = 1\n    _ = my_slice(a)\n}\n"),
        ("any to map", "type my_map = map[string]int\nfn main() {\n    var a any = 1\n    _ = my_map(a)\n}\n"),
        ("any to chan", "type my_chan = chan int\nfn main() {\n    var a any = 1\n    _ = my_chan(a)\n}\n"),
        ("any to struct", "struct holder {\n    n int\n}\nfn main() {\n    var a any = 1\n    _ = holder(a)\n}\n"),
    ];
    for (label, src) in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected reject for {label}");
        assert!(error_codes(&r).contains(&"HYB-E1002".to_string()), "expected HYB-E1002 for {label}, got: {:?}", r.diagnostics);
    }
}

#[test]
fn valid_conversions_to_interface_and_between_interfaces_accept() {
    let src = r#"
struct point {
    x int
}

fn main() {
    // Converting concrete types to interface is legal
    _ = any(42)
    _ = any("hello")
    _ = any(true)
    var p point
    _ = any(p)

    // Converting interface to interface is legal
    var a any = 100
    _ = any(a)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", r.diagnostics);
}
