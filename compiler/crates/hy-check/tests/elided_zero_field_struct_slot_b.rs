use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn elided_zero_field_struct_accepts_empty_and_rejects_unknown_field() {
    let ok_src = "struct empty {}\nfn main() {\n    xs := []empty{{}}\n    _ = xs\n}\n";
    let r_ok = check_file(&parse_file("ok_empty.hy", ok_src).unwrap());
    assert_eq!(r_ok.verdict, Verdict::Accept);

    let bad_src = "struct empty {}\nfn main() {\n    xs := []empty{{nope: 1}}\n    _ = xs\n}\n";
    let r_bad = check_file(&parse_file("bad_empty.hy", bad_src).unwrap());
    assert_eq!(r_bad.verdict, Verdict::Reject);
    assert!(r_bad.diagnostics.iter().any(|d| d.code == "HYB-E1002" && d.message.contains("unknown field `nope`")));
}
