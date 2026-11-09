use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn sync_waitgroup_fields_and_bindings_emit_and_resolve_3520() {
    let src = "struct s { wg sync.waitgroup\n wgp *sync.waitgroup }\nfn run(x *s) { var lwg sync.waitgroup\n x.wg.add(1)\n x.wgp.done()\n lwg.wait() }\n";
    let go = emit_go(&parse_file("test.hy", src).expect("parses")).expect("emits");
    assert!(go.contains("wg sync.WaitGroup") && go.contains("wgp *sync.WaitGroup") && go.contains("var lwg sync.WaitGroup") && go.contains("x.wg.Add(1)") && go.contains("x.wgp.Done()") && go.contains("lwg.Wait()"), "{go}");
}
