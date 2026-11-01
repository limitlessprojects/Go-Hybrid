use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check_codes(src: &str) -> (Verdict, Vec<String>) {
    let res = check_file(&parse_file("inline.hy", src).expect("parse"));
    (res.verdict, res.diagnostics.iter().map(|d| d.code.clone()).collect())
}

#[test]
fn forwarded_multi_return_calls_track_const_mutation_out_params_and_a3_loans() {
    for (src, exp_verdict, exp_code) in [
        ("const shared = map[string]int{\"k\": 1}\nfn pair_with_tag(m map[string]int) (int, map[string]int) { return 0, m }\nfn write_second(tag int, m map[string]int) { m[\"k\"] = tag + 99 }\nfn main() { write_second(pair_with_tag(shared)) }\n", Verdict::Reject, "HYB-E3006"),
        ("const shared = map[string]int{\"k\": 1}\nstruct writer { id int }\nfn (w writer) write_second(tag int, m map[string]int) { m[\"k\"] = w.id + tag }\nfn pair_const() (int, map[string]int) { return 7, shared }\nfn main() {\n w := writer{id: 1}\n w.write_second(pair_const())\n (fn(tag int, m map[string]int) { m[\"k\"] = tag })(pair_const())\n}\n", Verdict::Reject, "HYB-E3006"),
        ("fn pair_maps(dst map[string][]int, src []int) (map[string][]int, []int) { return dst, src }\nfn link_slot(dst map[string][]int, src []int) { dst[\"k\"] = src }\nfn consume(xs []int) { xs[0] = 99 }\nfn main() {\n dst := map[string][]int{}\n src := []int{1, 2, 3}\n link_slot(pair_maps(dst, src))\n task consume(move src)\n dst[\"k\"][0] = 42\n}\n", Verdict::Reject, "HYB-E3003"),
        ("struct entry { v int }\nfn pair_entry_map(tag int, m map[string]entry) (int, map[string]entry) { return tag, m }\nfn insert_second(tag int, m map[string]entry) { m[\"b\"] = entry{v: tag} }\nfn main() {\n m := map[string]entry{\"a\": entry{v: 1}}\n slot := &m[\"a\"]\n insert_second(pair_entry_map(2, m))\n slot.v = 99\n}\n", Verdict::Reject, "HYB-E3102"),
        ("const shared = map[string]int{\"k\": 1}\nfn pair_with_tag(m map[string]int) (int, map[string]int) { return 1, m }\nfn read_second(tag int, m map[string]int) int { return tag + m[\"k\"] }\nfn main() { _ = read_second(pair_with_tag(shared)) }\n", Verdict::Accept, ""),
    ] {
        let (verdict, codes) = check_codes(src);
        assert_eq!(verdict, exp_verdict, "{:?}", codes);
        if !exp_code.is_empty() {
            assert!(codes.iter().any(|c| c == exp_code), "expected {}, got {:?}", exp_code, codes);
        }
    }
}
