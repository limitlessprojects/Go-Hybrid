//! map_float_int_soundness.rs — regression tests for map indexing and map literals:
//! verifies that using typed int variables as float64 keys or values in map indexing
//! and map composite literals is rejected with HYB-E1002, while untyped integer
//! constants remain cleanly accepted (fixes #2679).

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
fn typed_int_var_in_map_float_index_read_rejected_with_e1002() {
    let src = "\
fn main() {
    m := map[float64]string{1.0: \"ok\"}
    var i int = 42
    _ = m[i]
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for map[float64] read with typed int index, got {:?}",
        r.diagnostics
    );
}

#[test]
fn typed_int_var_in_map_float_index_write_rejected_with_e1002() {
    let src = "\
fn main() {
    m := map[float64]string{1.0: \"ok\"}
    var i int = 42
    m[i] = \"bad\"
    _ = m
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for map[float64] write with typed int index, got {:?}",
        r.diagnostics
    );
}

#[test]
fn typed_int_var_in_map_lit_float_key_rejected_with_e1002() {
    let src = "\
fn main() {
    var i int = 42
    m := map[float64]string{i: \"bad\"}
    _ = m
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for map literal with typed int key, got {:?}",
        r.diagnostics
    );
}

#[test]
fn typed_int_var_in_map_lit_float_val_rejected_with_e1002() {
    let src = "\
fn main() {
    var i int = 42
    m := map[string]float64{\"k\": i}
    _ = m
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        error_codes(&r).contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 for map literal with typed int value, got {:?}",
        r.diagnostics
    );
}

#[test]
fn untyped_int_constants_and_map_float_accepted() {
    let src = "\
const file_k = 10

fn main() {
    const local_k = 20

    m := map[float64]string{1.0: \"ok\"}

    // Index read with constants
    _ = m[1]
    _ = m[local_k]
    _ = m[file_k]
    _ = m[-5]
    _ = m[1 + 2]

    // Index write with constants
    m[2] = \"two\"
    m[local_k] = \"twenty\"

    // Map literals with constants
    m2 := map[float64]string{
        1: \"one\",
        local_k: \"k\",
        file_k: \"fk\",
        -3: \"neg\",
    }

    m3 := map[string]float64{
        \"a\": 10,
        \"b\": local_k,
        \"c\": file_k,
        \"d\": 1 + 3,
    }

    _ = m
    _ = m2
    _ = m3
}
";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept for untyped integer constants in map float operations, got diags: {:?}",
        r.diagnostics
    );
}
