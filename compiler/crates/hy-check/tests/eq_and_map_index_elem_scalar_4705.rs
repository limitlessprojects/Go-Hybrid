//! eq_and_map_index_elem_scalar_4705.rs — regression for issue #4705:
//! equality (`==`/`!=`) on pointers, channels, and arrays with different
//! inner scalar widths, and map indexing with mismatched structural key
//! types (`map[[4]int]`, `map[*int32]`), bypassed the checks because only
//! the outer OpKind was compared.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn has_e1002(r: &hy_check::CheckResult) -> bool {
    r.diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error && d.code == "HYB-E1002")
}

fn diag_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn pointer_eq_width_mismatch_rejects() {
    let r = check(
        "fn main() {\n    var p32 *int32\n    var p64 *int64\n    _ = p32 == p64\n}\n",
    );
    assert!(has_e1002(&r), "got {:?}", diag_codes(&r));
    assert_eq!(r.verdict, Verdict::Reject);
}

#[test]
fn pointer_ne_width_mismatch_rejects() {
    let r = check(
        "fn main() {\n    var p32 *int32\n    var p64 *int64\n    _ = p32 != p64\n}\n",
    );
    assert!(has_e1002(&r), "got {:?}", diag_codes(&r));
}

#[test]
fn chan_eq_width_mismatch_rejects() {
    let r = check(
        "fn main() {\n    var ch32 chan int32\n    var ch64 chan int64\n    _ = ch32 == ch64\n}\n",
    );
    assert!(has_e1002(&r), "got {:?}", diag_codes(&r));
}

#[test]
fn array_eq_width_mismatch_rejects() {
    let r = check(
        "fn main() {\n    var a32 [4]int32\n    var a64 [4]int64\n    _ = a32 == a64\n}\n",
    );
    assert!(has_e1002(&r), "got {:?}", diag_codes(&r));
}

#[test]
fn map_index_array_key_size_mismatch_rejects() {
    let r = check(
        "fn main() {\n    var m map[[4]int]string\n    var k [3]int\n    _ = m[k]\n}\n",
    );
    assert!(has_e1002(&r), "got {:?}", diag_codes(&r));
}

#[test]
fn map_index_pointer_key_width_mismatch_rejects() {
    let r = check(
        "fn main() {\n    var m map[*int32]string\n    var p64 *int64\n    _ = m[p64]\n}\n",
    );
    assert!(has_e1002(&r), "got {:?}", diag_codes(&r));
}

#[test]
fn same_type_eq_and_index_accept() {
    let r = check(
        "fn main() {\n    var p32 *int32\n    var p32b *int32\n    _ = p32 == p32b\n    var ch chan int32\n    var ch2 chan int32\n    _ = ch == ch2\n    var a [4]int32\n    var ab [4]int32\n    _ = a == ab\n    var m map[[4]int]string\n    var k [4]int\n    _ = m[k]\n    var m2 map[*int32]string\n    _ = m2[p32]\n}\n",
    );
    assert!(
        !has_e1002(&r),
        "expected same-type comparisons to accept, got {:?}",
        diag_codes(&r)
    );
}

#[test]
fn untyped_const_eq_still_accepts() {
    let r = check(
        "fn main() {\n    var n int32 = 5\n    _ = n == 5\n}\n",
    );
    assert!(!has_e1002(&r), "got {:?}", diag_codes(&r));
}

#[test]
fn pointer_nil_eq_still_accepts() {
    let r = check(
        "fn main() {\n    var p32 *int32\n    _ = p32 == nil\n}\n",
    );
    assert!(!has_e1002(&r), "got {:?}", diag_codes(&r));
}
