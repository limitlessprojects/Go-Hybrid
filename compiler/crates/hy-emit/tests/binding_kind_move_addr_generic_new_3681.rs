#[test]
fn binding_kind_move_addr_generic_new_3681() {
    let src = "import \"sync\"\nfn make_set[t]() set[t] {\n    return set[t]{}\n}\nfn main() {\n    mu := new(sync.mutex)\n    pmu := &mu\n    pmu.lock()\n    s1 := make_set[int]()\n    s2 := move s1\n    s3 := new(set[int])\n    _ = len(s2)\n    delete(s3, 1)\n}\n";
    let go = hy_emit::emit_go(&hy_syntax::parse_file("t.hy", src).unwrap()).unwrap();
    assert!(go.contains("pmu.Lock()") && go.contains("s2.len()") && go.contains("s3.remove(1)"), "{go}");
}