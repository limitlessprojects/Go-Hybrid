use hy_check::{check_file, Verdict};
use hy_emit::{emit_go, emit_go_with_plan};
use hy_syntax::parse_file;

#[test]
fn string_and_unknown_bases_remain_two_index_without_helpers() {
    for (source, unchanged) in [
        ("fn view(s string) string { return s[1:2] }", "return s[1:2]"),
        ("type text = string\nfn view(s text) text { return s[1:2] }", "return s[1:2]"),
        ("fn main() { _ = os.args()[0:1] }", "os.Args[0:1]"),
    ] {
        let file = parse_file("controls.hy", source).unwrap();
        let checked = check_file(&file);
        assert_eq!(checked.verdict, Verdict::Accept, "{:?}", checked.diagnostics);
        let go = emit_go_with_plan(&file, &checked.clip_plan).unwrap();
        assert!(go.contains(unchanged), "{go}");
        assert!(!go.contains("hy_clip"), "{go}");
    }
}

#[test]
fn checked_plan_is_explicit_and_preserves_single_evaluation() {
    let file = parse_file("slice.hy", "fn view(s []int, lo int, hi int) []int { return s[lo:hi] }").unwrap();
    let checked = check_file(&file);
    let planned = emit_go_with_plan(&file, &checked.clip_plan).unwrap();
    assert!(planned.contains(&format!("return {}(s[lo:hi])", checked.clip_plan.helper_name())), "{planned}");
    let unplanned = emit_go(&file).unwrap();
    assert!(unplanned.contains("return s[lo:hi]"), "{unplanned}");
    assert!(!unplanned.contains("hy_clip"), "{unplanned}");
}
