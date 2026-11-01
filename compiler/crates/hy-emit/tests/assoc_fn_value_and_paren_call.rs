use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn assoc_fn_value_and_parenthesized_call_mangle_to_go_ident() {
    let src = "struct env { r int }\nfn env.scale(e *env, k int) { e.r = e.r * k }\nfn main() { var e env\nf := env.scale\n(env.scale)(e, 2)\n_ = f }\n";
    let go = emit_go(&parse_file("t.hy", src).unwrap()).unwrap();
    assert!(go.contains("f := env_scale") && go.contains("(env_scale)(&e, 2)"), "{go}");
}
