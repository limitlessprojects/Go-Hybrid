//! array_conversion_soundness_e1002.rs — tests for HYB-E1002 rejection of
//! invalid type conversions between Array types ([N]T) and incompatible types
//! (scalars, strings, structs, maps, channels).
//! Issue #2479: SPEC-LANGUAGE §3.1 & GUARANTEES.md Row 13 (BUILDABLE).

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
fn array_conversion_from_int_literal_or_var_rejects_e1002() {
    for snippet in [
        "type my_arr = [4]int\nfn main() {\n    _ = my_arr(1)\n}\n",
        "type my_arr = [4]int\nfn main() {\n    x := 10\n    _ = my_arr(x)\n}\n",
    ] {
        let r = check(snippet);
        assert_eq!(r.verdict, Verdict::Reject);
        assert!(error_codes(&r).contains(&"HYB-E1002".to_string()), "expected HYB-E1002 for: {snippet}");
    }
}

#[test]
fn int_conversion_from_array_rejects_e1002() {
    let src = "type my_arr = [4]int\nfn main() {\n    var a my_arr\n    _ = int(a)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
}

#[test]
fn float_conversion_to_and_from_array_rejects_e1002() {
    let cases = [
        "type my_arr = [4]int\nfn main() {\n    f := 3.14\n    _ = my_arr(f)\n}\n",
        "type my_arr = [4]int\nfn main() {\n    var a my_arr\n    _ = float64(a)\n}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject);
        assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    }
}

#[test]
fn string_conversion_to_and_from_array_rejects_e1002() {
    let cases = [
        "type my_arr = [4]byte\nfn main() {\n    s := \"abc\"\n    _ = my_arr(s)\n}\n",
        "type my_arr = [4]byte\nfn main() {\n    var a my_arr\n    _ = string(a)\n}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject);
        assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    }
}

#[test]
fn struct_and_composite_conversion_with_array_rejects_e1002() {
    let cases = [
        "type my_arr = [2]int\nstruct point {\n    x int\n    y int\n}\nfn main() {\n    var p point\n    _ = my_arr(p)\n}\n",
        "type my_arr = [2]int\nstruct point {\n    x int\n    y int\n}\nfn main() {\n    var a my_arr\n    _ = point(a)\n}\n",
        "type my_arr = [2]int\ntype my_map = map[string]int\nfn main() {\n    var a my_arr\n    _ = my_map(a)\n}\n",
        "type my_arr = [2]int\ntype my_chan = chan int\nfn main() {\n    var a my_arr\n    _ = my_chan(a)\n}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject);
        assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    }
}

#[test]
fn valid_array_operations_and_interface_conversions_accept() {
    let src = r#"
type my_arr = [3]int

fn main() {
    var a my_arr
    _ = a

    // Converting array to interface is valid in Go
    _ = any(a)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected accept, got diagnostics: {:?}", r.diagnostics);
}
