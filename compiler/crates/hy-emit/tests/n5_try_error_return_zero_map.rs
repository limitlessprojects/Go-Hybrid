use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn n5_try_error_propagation_allocates_zero_maps_instead_of_nil() {
    let src = "struct bag { counts map[string]int }\nfn step() (int, error) { return 1, nil }\nfn load_m() (map[string]int, error) { _ = step()?\nreturn map[string]int{}, nil }\nfn load_b() (bag, error) { _ = step()?\nreturn bag{}, nil }\n";
    let go = emit_go(&parse_file("t.hy", src).unwrap()).unwrap();
    assert!(go.contains("return map[string]int{}, hyErr0"), "{go}");
    assert!(go.contains("return bag{counts: map[string]int{}}, hyErr1"), "{go}");
}
