//! Regression tests for #4186:
//! Curated foreign package function calls (`bytes.*`) must enforce call arity,
//! reject non-variadic spread arguments, and reject non-callable package symbols
//! during `hyb check` (HYB-E1002, Row 13 BUILDABLE).

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

fn assert_no_e1002(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E1002"),
        "expected no HYB-E1002 for {:?}, got {:?}",
        src,
        cs
    );
}

fn accepts(src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "verdict for {:?}: {:?}", src, r.diagnostics);
}

#[test]
fn bytes_non_callable_symbols_rejected_e1002() {
    let cases = [
        "import \"bytes\"\nfn main() {\n    _ = bytes.buffer()\n}\n",
        "import \"bytes\"\nfn main() {\n    _ = bytes.reader()\n}\n",
        "import \"bytes\"\nfn main() {\n    _ = bytes.min_read()\n}\n",
        "import \"bytes\"\nfn main() {\n    _ = bytes.err_too_large()\n}\n",
    ];
    for src in cases {
        rejects_e1002(src);
    }
}

#[test]
fn bytes_call_arity_mismatch_rejected_e1002() {
    let cases = [
        "import \"bytes\"\nfn main() {\n    _ = bytes.new_buffer()\n}\n",
        "import \"bytes\"\nfn main() {\n    b := []byte{1}\n    _ = bytes.new_buffer(b, b)\n}\n",
        "import \"bytes\"\nfn main() {\n    _ = bytes.new_reader()\n}\n",
        "import \"bytes\"\nfn main() {\n    _ = bytes.trim_space()\n}\n",
        "import \"bytes\"\nfn main() {\n    b := []byte{1}\n    _ = bytes.trim_space(b, b)\n}\n",
        "import \"bytes\"\nfn main() {\n    _ = bytes.fields()\n}\n",
        "import \"bytes\"\nfn main() {\n    _ = bytes.runes()\n}\n",
        "import \"bytes\"\nfn main() {\n    b := []byte{1}\n    _ = bytes.contains(b)\n}\n",
        "import \"bytes\"\nfn main() {\n    _ = bytes.split()\n}\n",
        "import \"bytes\"\nfn main() {\n    b := []byte{1}\n    _ = bytes.equal_fold(b)\n}\n",
        "import \"bytes\"\nfn main() {\n    b := []byte{1}\n    _ = bytes.equal_fold(b, b, b)\n}\n",
        "import \"bytes\"\nfn main() {\n    b := []byte{1}\n    _ = bytes.replace_all(b, b)\n}\n",
        "import \"bytes\"\nfn main() {\n    b := []byte{1}\n    _ = bytes.replace(b, b, b)\n}\n",
    ];
    for src in cases {
        rejects_e1002(src);
    }
}

#[test]
fn bytes_non_variadic_spread_rejected_e1002() {
    let cases = [
        "import \"bytes\"\nfn main() {\n    xs := []byte{1, 2}\n    _ = bytes.to_lower(xs...)\n}\n",
        "import \"bytes\"\nfn main() {\n    xs := []byte{1, 2}\n    _ = bytes.split(xs, xs...)\n}\n",
    ];
    for src in cases {
        rejects_e1002(src);
    }
}

#[test]
fn bytes_positive_controls_do_not_emit_e1002() {
    // Note: foreign.rs currently keeps `"bytes"` in both `is_curated_import`
    // and `is_uncurated_go_stdlib_root`, so resolve.rs still emits HYB-E1001.
    // Assert that valid-arity `bytes.*` calls never spuriously emit HYB-E1002.
    assert_no_e1002(
        r#"
import "bytes"

fn test() {
    b1 := []byte{65, 66}
    b2 := []byte{66}
    _ = bytes.new_buffer(b1)
    _ = bytes.new_buffer_string("ab")
    _ = bytes.new_reader(b1)
    _ = bytes.fields(b1)
    _ = bytes.runes(b1)
    _ = bytes.to_lower(b1)
    _ = bytes.trim_space(b1)
    _ = bytes.contains(b1, b2)
    _ = bytes.has_prefix(b1, b2)
    _ = bytes.equal(b1, b2)
    _ = bytes.equal_fold(b1, b2)
    _ = bytes.split(b1, b2)
    _ = bytes.replace_all(b1, b2, b1)
    _ = bytes.replace(b1, b2, b1, 1)
}
"#,
    );
}

#[test]
fn shadowed_bytes_binding_not_intercepted() {
    accepts(
        r#"
fn bytes_shadow() {
    bytes := 123
    _ = bytes
}
"#,
    );
}
