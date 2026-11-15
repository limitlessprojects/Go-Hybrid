use hy_check::check_file;
use hy_syntax::parse_file;
fn has_code(src: &str, code: &str) -> bool { check_file(&parse_file("test.hy", src).unwrap()).diagnostics.iter().any(|d| d.code == code) }
#[test]
fn generic_struct_nameless_fn_param_rejects_overflow_and_chan_dir_and_accepts_valid() {
    assert!(has_code("struct holder[t] { cb fn(t) }\nfn bad(h holder[u8]) { h.cb(300) }\n", "HYB-E6002"));
    assert!(has_code("struct holder[t] { cbs []fn(t) }\nfn bad(h holder[u8]) { h.cbs[0](300) }\n", "HYB-E6002"));
    assert!(has_code("type send_ch = chan<- int\nstruct holder[t] { cb fn(t) }\nfn bad(h holder[send_ch], ro <-chan int) { h.cb(ro) }\n", "HYB-E3012"));
    assert!(!has_code("struct holder[t] { cb fn(t) }\nfn ok(h holder[u8]) { h.cb(200) }\n", "HYB-E6002"));
}
