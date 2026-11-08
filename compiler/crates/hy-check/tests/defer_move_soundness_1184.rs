use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn error_codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("parse");
    check_file(&ast).diagnostics.iter().filter(|d| d.severity == Severity::Error).map(|d| d.code.clone()).collect()
}

#[test]
fn defer_arg_select_and_field_move_send_and_range_peer_reject_e3003() {
    let c1 = error_codes("fn report(m map[string]int) int { return m[\"task\"] }\nfn main() {\n ch := make(chan map[string]int, 2)\n if true {\n shared := map[string]int{\"task\": 1}\n defer report(shared)\n select { ch <- move shared: }\n }\n top := map[string]int{\"task\": 2}\n alias := top\n defer report(top)\n top = map[string]int{}\n select { ch <- move alias: }\n}\n");
    assert_eq!(c1, vec!["HYB-E3003".to_string(), "HYB-E3003".to_string()]);

    let c2 = error_codes("struct box { shared map[string]int }\nfn report(m map[string]int) int { return m[\"task\"] }\nfn consume(m map[string]int) { m[\"task\"]++ }\nfn main() {\n ch := make(chan map[string]int, 1)\n if true {\n b := box{shared: map[string]int{\"task\": 1}}\n defer report(b.shared)\n ch <- move b.shared\n }\n if true {\n s := map[string]int{\"task\": 2}\n defer report(s)\n consume(move s)\n }\n}\n");
    assert_eq!(c2.iter().filter(|c| *c == "HYB-E3003").count(), 2);

    let c3 = error_codes("fn report(m map[string]int) int { return m[\"task\"] }\nfn worker(m map[string]int) { m[\"task\"]++ }\nfn main() {\n items := []map[string]int{map[string]int{\"task\": 1}}\n defer report(items[0])\n for _, v in items {\n items = []map[string]int{}\n task worker(move v)\n }\n}\n");
    assert!(c3.contains(&"HYB-E3003".to_string()), "got {:?}", c3);
}
