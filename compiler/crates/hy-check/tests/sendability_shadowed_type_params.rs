use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    check_file(&hy_syntax::parse_file("test.hy", src).expect("test source must parse"))
}

fn assert_rejects(src: &str, code: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject ({code}) for:\n{src}\ndiags: {:?}", r.diagnostics);
    assert!(r.diagnostics.iter().any(|d| d.code == code), "expected {code}, got {:?}", r.diagnostics);
}

#[test]
fn generic_type_params_shadowing_aliases_enums_structs_or_predeclared_types_reject_boundary_sends() {
    assert_rejects("type t = int\nfn dup[t](x t, c1 chan<- t, c2 chan<- t) { c1 <- x\n c2 <- x }", "HYB-E3005");
    assert_rejects("enum t { a\n b }\nfn dup[t](x t, ch chan<- t) { ch <- x }", "HYB-E3005");
    assert_rejects("struct t { v int }\nfn dup[t](x t, ch chan<- t) { const y = x\n ch <- y }", "HYB-E3005");
    assert_rejects("fn dup[int](x int, c1 chan<- int, c2 chan<- int) { c1 <- x\n c2 <- x }", "HYB-E3005");
    assert_rejects("fn dup_conv[int](x int, c1 chan<- int, c2 chan<- int) { c1 <- int(x)\n c2 <- int(x) }", "HYB-E3005");
    assert_rejects("fn dup_str[string](x string, ch chan<- string) { ch <- x }", "HYB-E3005");
    assert_rejects("type t = int\nstruct holder[t] { val t }\nfn (h *holder[t]) dup(c1 chan<- t, c2 chan<- t) { c1 <- h.val\n c2 <- h.val }", "HYB-E3005");
    assert_rejects("struct holder[int] { val int }\nfn (h *holder[int]) dup(c1 chan<- int, c2 chan<- int) { c1 <- h.val\n c2 <- h.val }", "HYB-E3005");
    assert_rejects("type t = int\nfn smuggle_param[t](x t, ch chan<- t) { ch <- move x }", "HYB-E3004");
    assert_rejects("type t = int\nfn smuggle_local[t](ch <-chan t, out chan<- t) { v := <-ch\n out <- move v }", "HYB-E3005");
    assert_rejects("type t = int\nfn recv_dup[t](ch <-chan t, c1 chan<- t, c2 chan<- t) { v, ok := <-ch\n if ok { c1 <- v\n c2 <- v } }", "HYB-E3005");
    assert_rejects("type t = int\nfn opt_dup[t](o option[t], ch chan<- t) { v := o.unwrap()\n ch <- v }", "HYB-E3005");
    assert_rejects("type t = int\nfn clos_dup[t](ch chan<- t) { f := fn(x t) { ch <- x }\n _ = f }", "HYB-E3005");
}

#[test]
fn generic_calls_and_instantiated_containers_with_shadowed_type_params_fail_closed() {
    assert_rejects("type t = int\nstruct box[t] { v t }\nfn wrap_box[t](x t) box[t] { return box[t]{v: x} }\nfn sink_any(v any) { _ = v }\nfn main() {\n m := map[string]int{\"k\": 1}\n b := wrap_box(m)\n task sink_any(b)\n}", "HYB-E3001");
    assert_rejects("type t = int\nstruct box[t] { v t }\nfn main() {\n ch := make(chan box[map[string]int], 1)\n b := box[map[string]int]{v: map[string]int{\"k\": 1}}\n ch <- b\n}", "HYB-E3002");
    assert_rejects("type t = int\nfn make_zero[u]() u { var z u\n return z }\nfn work[t](ch chan<- t) { v := make_zero[t]()\n ch <- v }", "HYB-E3005");
    assert_rejects("type t = int\nstruct holder[u] { v u }\nfn leak_holder[t](x t, ch chan<- holder[t]) { h := holder[t]{v: x}\n ch <- move h }", "HYB-E3005");
    assert_rejects("type t = int\nfn leak_set[t](s set[t], ch chan<- set[t]) { _ = s\n var loc set[t]\n ch <- move loc }", "HYB-E3005");
}

#[test]
fn positive_controls_preserve_concrete_types_and_channel_handles_under_shadowed_type_params() {
    let src = "type t = int\ntype u = fn()\nstruct plain { x t }\nstruct mixed[v] { fixed t\n param v }\nstruct box[t] { v t }\n\
               fn wrap_box[t](x t) box[t] { return box[t]{v: x} }\nfn id_u[u](x u) u { return x }\nfn sink_any(v any) { _ = v }\n\
               fn forward_chan[t](target chan<- t, meta chan<- chan<- t) { meta <- target }\n\
               fn send_concrete[t](p plain, m mixed[t], out chan<- int) { out <- p.x\n out <- m.fixed }\n\
               fn main() {\n ch := make(chan box[map[string]int], 1)\n b1 := wrap_box(map[string]int{\"k\": 1})\n task sink_any(move b1)\n\
               b2 := box[map[string]int]{v: map[string]int{\"k\": 2}}\n ch <- move b2\n n := id_u(42)\n task sink_any(move n)\n}";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept, diags: {:?}", r.diagnostics);
}
