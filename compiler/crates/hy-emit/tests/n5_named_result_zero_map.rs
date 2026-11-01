use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn n5_named_results_allocate_zero_maps_and_map_carrying_structs() {
    let src = "struct bag { counts map[string]int }\nfn build_m() (m map[string]int) { m[\"a\"] = 1\nreturn }\nfn build_b() (b bag) { b.counts[\"k\"] = 2\nreturn }\n";
    let go = emit_go(&parse_file("t.hy", src).unwrap()).unwrap();
    assert!(go.contains("m = map[string]int{}"), "{go}");
    assert!(go.contains("b = bag{counts: map[string]int{}}"), "{go}");
}
