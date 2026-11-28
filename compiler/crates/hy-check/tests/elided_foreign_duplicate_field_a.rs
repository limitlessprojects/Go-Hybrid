use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn elided_foreign_struct_duplicate_field_rejected_e1002() {
    let src = "import \"net/http\"\nfn main() {\n    xs := []http.server{{addr: \":80\", addr: \":81\"}}\n    _ = xs\n}\n";
    let r = check_file(&parse_file("repro_elided_dup.hy", src).unwrap());
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E1002" && d.message.contains("duplicate field `addr`")));
}
