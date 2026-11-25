use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn wait_group_and_once_struct_fields_emit_exported_go_methods_3512() {
    let src = "struct runner {\n    wg sync.wait_group\n    once sync.once\n}\nfn main() {\n    var r runner\n    r.wg.add(1)\n    r.wg.done()\n    r.wg.wait()\n    r.once.do(fn() {})\n}\n";
    let go = emit_go(&parse_file("test.hy", src).expect("parses")).expect("emits");
    for expected in ["r.wg.Add(1)", "r.wg.Done()", "r.wg.Wait()", "r.once.Do("] {
        assert!(go.contains(expected), "missing `{expected}` in:\n{go}");
    }
}