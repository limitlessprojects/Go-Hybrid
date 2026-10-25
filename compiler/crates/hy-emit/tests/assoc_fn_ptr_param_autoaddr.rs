use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn assoc_fn_call_auto_addresses_addressable_arg_at_pointer_param() {
    let src = "struct env { r int }\nfn env.scale(e *env, k int) { e.r = e.r * k }\nfn main() { var e env\nenv.scale(e, 3) }\n";
    let go = emit_go(&parse_file("t.hy", src).unwrap()).unwrap();
    assert!(go.contains("env_scale(&e, 3)"), "{go}");
}
