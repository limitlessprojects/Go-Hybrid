#[test]
fn emit_new_translates_foreign_and_generic_types_and_preserves_shapes_3669() {
    let src = "import \"sync\"\nstruct box[t] { mu sync.mutex\n val t }\nfn main() {\n new(sync.mutex).lock()\n _ = new(set[u8])\n b := new(box[u8])\n b.mu.lock()\n}\n";
    let go = hy_emit::emit_go(&hy_syntax::parse_file("test.hy", src).unwrap()).unwrap();
    assert!(go.contains("new(sync.Mutex).Lock()") && go.contains("new(set[uint8])") && go.contains("new(box[uint8])") && go.contains("b.mu.Lock()"), "{go}");
}
