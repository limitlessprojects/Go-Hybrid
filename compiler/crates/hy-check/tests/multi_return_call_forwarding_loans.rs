use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    check_file(&parse_file("inline.hy", src).expect("parse")).diagnostics.into_iter().map(|d| d.code).collect()
}

#[test]
fn forwarded_multi_return_calls_track_const_mutation_and_out_params() {
    assert_eq!(codes("const shared = map[string]int{\"k\": 1}\nfn pair_with_tag(m map[string]int) (int, map[string]int) { return 0, m }\nfn write_second(tag int, m map[string]int) { m[\"k\"] = tag + 99 }\nfn main() { write_second(pair_with_tag(shared)) }\n"), vec!["HYB-E3006"]);
    assert!(codes("fn pair_maps(dst map[string][]int, src []int) (map[string][]int, []int) { return dst, src }\nfn link_slot(dst map[string][]int, src []int) { dst[\"k\"] = src }\nfn consume(xs []int) { xs[0] = 99 }\nfn main() {\n dst := map[string][]int{}\n src := []int{1, 2, 3}\n link_slot(pair_maps(dst, src))\n task consume(move src)\n dst[\"k\"][0] = 42\n}\n").contains(&"HYB-E3003".to_string()));
    assert!(codes("const shared = map[string]int{\"k\": 1}\nfn pair_with_tag(m map[string]int) (int, map[string]int) { return 1, m }\nfn read_second(tag int, m map[string]int) int { return tag + m[\"k\"] }\nfn main() { _ = read_second(pair_with_tag(shared)) }\n").is_empty());
}
