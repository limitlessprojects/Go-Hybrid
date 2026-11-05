use hy_check::{check_file, Verdict};
#[test]
fn check_call_const_overflow_checks_set_delete_and_fn_valued_exprs_3644() {
    let base = "type byte_set = set[u8]\nstruct holder[t] { cb fn(x t) }\nfn get_cb() fn(u8) { return fn(x u8) { _ = x } }\n";
    for body in ["s := set[u8]{}\n delete(s, 300)", "var s byte_set\n delete(s, 300)", "var h holder[u8]\n h.cb(300)", "var cbs []fn(u8)\n cbs[0](300)", "get_cb()(300)"] {
        let r = check_file(&hy_syntax::parse_file("test.hy", &format!("{base}fn main() {{ {body} }}\n")).unwrap());
        assert!(r.verdict == Verdict::Reject && r.diagnostics.iter().any(|d| d.code == "HYB-E6002"), "{body}: {:?}", r.diagnostics);
    }
}
