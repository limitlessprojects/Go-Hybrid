use hy_check::{check_file, Verdict};
#[test]
fn string_conv_2_512_radix_bounds() {
    for (lit, v) in [("0x1".to_owned() + &"0".repeat(128), Verdict::Reject), ("0x".to_owned() + &"f".repeat(128), Verdict::Accept), ("0b1".to_owned() + &"0".repeat(155), Verdict::Accept), ("0b1".to_owned() + &"0".repeat(512), Verdict::Reject), ("0o3".to_owned() + &"7".repeat(170), Verdict::Accept), ("0o4".to_owned() + &"0".repeat(170), Verdict::Reject)] {
        let r = check_file(&hy_syntax::parse_file("t.hy", &format!("fn main() {{ s := string({lit})\nprintln(s) }}")).unwrap());
        assert_eq!((r.verdict, r.diagnostics.iter().any(|d| d.code == "HYB-E6006")), (v, v == Verdict::Reject), "{lit}");
    }
}
