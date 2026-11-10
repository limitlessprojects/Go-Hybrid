//! issue_1587_const_conversions.rs — regression for issue #1587:
//! const string(65), len(string(65)), and mismatched scalar const conversions
//! pass hyb check and fail go build.

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "test_1587.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn integer_to_string_conversion_in_const_is_rejected_e6003() {
    for src in [
        "const s = string(65)\nfn main() {}\n",
        "type my_str = string\nconst s = my_str(65)\nfn main() {}\n",
        "fn main() {\n    const s = string(65)\n}\n",
        "const n = len(string(65))\nfn main() {}\n",
        "const n = len(string(true))\nfn main() {}\n",
    ] {
        let r = check(src);
        let cs = error_codes(&r);
        assert!(
            cs.iter().any(|c| c == "HYB-E6003"),
            "expected HYB-E6003 for {src}, got {cs:?}"
        );
    }
}

#[test]
fn cross_family_scalar_conversions_in_const_are_rejected_e6003() {
    for src in [
        "const b = bool(1)\nfn main() {}\n",
        "const b = bool(\"ok\")\nfn main() {}\n",
        "const x = int(\"ok\")\nfn main() {}\n",
        "const x = int(true)\nfn main() {}\n",
        "const f = float64(\"ok\")\nfn main() {}\n",
        "const s = string(true)\nfn main() {}\n",
        "fn main() {\n    const b = bool(1)\n    const x = int(\"ok\")\n}\n",
    ] {
        let r = check(src);
        let cs = error_codes(&r);
        assert!(
            cs.iter().any(|c| c == "HYB-E6003"),
            "expected HYB-E6003 for {src}, got {cs:?}"
        );
    }
}

#[test]
fn cyclic_string_const_recursion_safely_terminates() {
    let src = "const a = b + b\nconst b = a + a\nconst n = len(a)\nfn main() {}\n";
    let r = check(src);
    let cs = error_codes(&r);
    // Mutually cyclic const definition cannot evaluate to a constant expression
    assert!(
        cs.iter().any(|c| c == "HYB-E6003"),
        "expected HYB-E6003 for cyclic string const, got {cs:?}"
    );
}

#[test]
fn valid_constant_conversions_are_accepted() {
    for src in [
        "const s = string(\"ok\")\nfn main() {}\n",
        "type my_str = string\nconst s = my_str(\"hello\")\nfn main() {}\n",
        "const b = bool(true)\nfn main() {}\n",
        "const x = int8(42)\nfn main() {}\n",
        "const f = f64(42)\nfn main() {}\n",
        "const c = byte('A')\nfn main() {}\n",
        "const n = len(\"ok\")\nfn main() {}\n",
        "const n = len(string(\"ok\"))\nfn main() {}\n",
    ] {
        let r = check(src);
        let cs = error_codes(&r);
        assert!(
            !cs.iter().any(|c| c == "HYB-E6003"),
            "valid constant expression must NOT fire HYB-E6003 for {src}, got {cs:?}"
        );
    }
}
