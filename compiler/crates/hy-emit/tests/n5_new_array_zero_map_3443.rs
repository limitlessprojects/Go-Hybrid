use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn n5_new_array_and_alias_allocate_map_elements_3443() {
    let src = "struct bag {\n    counts map[string]int\n}\ntype two_bags = [2]bag\nfn main() {\n    p := new([2]bag{})\n    q := new((two_bags))\n    fmt.println(p, q)\n}\n";
    let ast = parse_file("test.hy", src).expect("parses");
    let go = emit_go(&ast).expect("emits");
    assert!(go.contains("p := func() *[2]bag { hyPtr := func() (hyArr [2]bag) { for i := range hyArr { hyArr[i] = bag{counts: map[string]int{}} }; return hyArr }(); return &hyPtr }()"), "{go}");
    assert!(go.contains("q := func() *two_bags { hyPtr := func() (hyArr two_bags) { for i := range hyArr { hyArr[i] = bag{counts: map[string]int{}} }; return hyArr }(); return &hyPtr }()"), "{go}");
    assert!(!go.contains("&func()"), "{go}");
}
