use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn n5_new_array_and_alias_allocate_map_elements_3443() {
    let src = "struct bag {\n    counts map[string]int\n}\ntype two_bags = [2]bag\nfn main() {\n    p := new([2]bag{})\n    q := new((two_bags))\n    fmt.println(p, q)\n}\n";
    let ast = parse_file("test.hy", src).expect("parses");
    let go = emit_go(&ast).expect("emits");
    assert!(go.contains("p := func() *[2]bag { var hyArr [2]bag; for hyIdx := range hyArr { hyArr[hyIdx] = bag{counts: map[string]int{}} }; return &hyArr }()"), "{go}");
    assert!(go.contains("q := func() *two_bags { var hyArr two_bags; for hyIdx := range hyArr { hyArr[hyIdx] = bag{counts: map[string]int{}} }; return &hyArr }()"), "{go}");
    assert!(!go.contains("&func()"), "{go}");
}
