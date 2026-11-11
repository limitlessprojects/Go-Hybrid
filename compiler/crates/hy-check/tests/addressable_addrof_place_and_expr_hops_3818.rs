//! Regression tests: `addressable.rs` must peel `PlaceKind::AddrOf` and
//! `as_addr_of` hops when checking assignment targets and O8 slot loan
//! operands so `&m["a"].n = 1`, `&s[0] = 65`, `&(&m["a"]).n`,
//! `&(&get_holder()).n`, and `&(&s)[0]` reject with `HYB-E2010`.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).expect("must parse");
    check_file(&ast)
}

fn assert_has_e2010(src: &str) {
    let r = check(src);
    let cs: Vec<_> = r.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.code.as_str()).collect();
    assert!(cs.contains(&"HYB-E2010"), "expected HYB-E2010, got {cs:?} for:\n{src}");
    assert_eq!(r.verdict, Verdict::Reject);
}

fn assert_accept(src: &str) {
    let r = check(src);
    let cs: Vec<_> = r.diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.code.as_str()).collect();
    assert!(cs.is_empty(), "expected 0 errors, got {cs:?} for:\n{src}");
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn addrof_place_and_expr_hops_on_map_call_and_string_reject_e2010() {
    assert_has_e2010(
        "struct holder { n int }\nfn bad(m map[string]holder) {\n    &m[\"a\"].n = 1\n}\nfn main() {}\n",
    );
    assert_has_e2010(
        "fn bad(s string) {\n    &s[0] = 65\n}\nfn main() {}\n",
    );
    assert_has_e2010(
        "struct holder { n int }\nfn bad(m map[string]holder) {\n    n := &(&m[\"a\"]).n\n    _ = n\n}\nfn main() {}\n",
    );
    assert_has_e2010(
        "struct holder { n int }\nfn get_holder() holder { return holder{n: 1} }\nfn main() {\n    n := &(&get_holder()).n\n    _ = n\n}\n",
    );
    assert_has_e2010(
        "fn bad(s string) {\n    b := &(&s)[0]\n    _ = b\n}\nfn main() {}\n",
    );
}

#[test]
fn addrof_expr_hops_on_addressable_local_accept() {
    assert_accept(
        "struct holder { n int }\nfn main() {\n    var h = holder{n: 1}\n    n := &(&h).n\n    _ = n\n    _ = h\n}\n",
    );
}
