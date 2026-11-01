//! chan_direction_coercion.rs — HYB-E3012: channel direction checking at
//! value-flow coercion sites (issue #623).
//!
//! Go's implicit channel direction conversion is one-way: bidirectional `chan T`
//! may narrow to `chan<- T` or `<-chan T`, whereas unidirectional channels may
//! never convert to the opposite direction or widen back to `chan T`.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

// ---------------------------------------------------------------------------
// Issue #623 Repro 1: Var-decl ascription, cross-unidir (`var rc <-chan int = sc`)
// ---------------------------------------------------------------------------
#[test]
fn repro_01_var_ascription_cross_unidir_rejects_e3012() {
    let r = check(
        "fn main() {\n\
           var sc chan<- int = make(chan int, 1)\n\
           var rc <-chan int = sc\n\
           _ = rc\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(error_codes(&r), vec!["HYB-E3012"]);
}

// ---------------------------------------------------------------------------
// Issue #623 Repro 2: Var-decl ascription, widening back to bidir (`var bc chan int = sc`)
// ---------------------------------------------------------------------------
#[test]
fn repro_02_var_ascription_widen_to_bidir_rejects_e3012() {
    let r = check(
        "fn main() {\n\
           var sc chan<- int = make(chan int, 1)\n\
           var bc chan int = sc\n\
           _ = bc\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(error_codes(&r), vec!["HYB-E3012"]);

    let r2 = check(
        "fn main() {\n\
           var rc <-chan int = make(chan int, 1)\n\
           var bc chan int = rc\n\
           _ = bc\n\
         }",
    );
    assert_eq!(r2.verdict, Verdict::Reject, "{:?}", r2.diagnostics);
    assert_eq!(error_codes(&r2), vec!["HYB-E3012"]);
}

// ---------------------------------------------------------------------------
// Issue #623 Repro 3: Assignment (`rc = sc`)
// ---------------------------------------------------------------------------
#[test]
fn repro_03_assignment_cross_unidir_rejects_e3012() {
    let r = check(
        "fn main() {\n\
           var sc chan<- int = make(chan int, 1)\n\
           var rc <-chan int\n\
           rc = sc\n\
           _ = rc\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(error_codes(&r), vec!["HYB-E3012"]);
}

// ---------------------------------------------------------------------------
// Issue #623 Repro 4: Call argument (`take_rc(sc)` where param is `<-chan int`)
// ---------------------------------------------------------------------------
#[test]
fn repro_04_call_arg_direction_mismatch_rejects_e3012() {
    let r = check(
        "fn take_rc(rc <-chan int) {\n\
           var v = <-rc\n\
           _ = v\n\
         }\n\
         fn main() {\n\
           var sc chan<- int = make(chan int, 1)\n\
           take_rc(sc)\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(error_codes(&r), vec!["HYB-E3012"]);
}

// ---------------------------------------------------------------------------
// Issue #623 Repro 5: Return statement (`return sc` from fn returning `<-chan int`)
// ---------------------------------------------------------------------------
#[test]
fn repro_05_return_direction_mismatch_rejects_e3012() {
    let r = check(
        "fn get_rc() <-chan int {\n\
           var sc chan<- int = make(chan int, 1)\n\
           return sc\n\
         }\n\
         fn main() {\n\
           var rc = get_rc()\n\
           _ = rc\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(error_codes(&r), vec!["HYB-E3012"]);
}

// ---------------------------------------------------------------------------
// Issue #623 Repro 6: Struct-literal field (`holder{ch: sc}`)
// ---------------------------------------------------------------------------
#[test]
fn repro_06_struct_lit_field_direction_mismatch_rejects_e3012() {
    let r = check(
        "struct holder {\n\
           ch <-chan int\n\
         }\n\
         fn main() {\n\
           var sc chan<- int = make(chan int, 1)\n\
           h := holder{ch: sc}\n\
           _ = h\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(error_codes(&r), vec!["HYB-E3012"]);
}

// ---------------------------------------------------------------------------
// Issue #623 Repro 7: Map store (`m["a"] = ch` where map value is `<-chan int`)
// ---------------------------------------------------------------------------
#[test]
fn repro_07_map_store_direction_mismatch_rejects_e3012() {
    let r = check(
        "fn main() {\n\
           var m = map[string]<-chan int{}\n\
           var ch chan<- int = make(chan int, 1)\n\
           m[\"a\"] = ch\n\
           var v = <-m[\"a\"]\n\
           _ = v\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(error_codes(&r), vec!["HYB-E3012"]);
}

// ---------------------------------------------------------------------------
// Issue #623 Repro 8: `task` spawn argument (`task work(sc)`)
// ---------------------------------------------------------------------------
#[test]
fn repro_08_task_spawn_arg_direction_mismatch_rejects_e3012() {
    let r = check(
        "fn work(rc <-chan int) {\n\
           var v = <-rc\n\
           _ = v\n\
         }\n\
         fn main() {\n\
           var sc chan<- int = make(chan int, 1)\n\
           task work(sc)\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(error_codes(&r), vec!["HYB-E3012"]);
}

// ---------------------------------------------------------------------------
// Issue #623 Repro 9: Closure argument + closure return (`run(sc)`, `return sc`)
// ---------------------------------------------------------------------------
#[test]
fn repro_09_closure_arg_and_return_direction_mismatch_reject_e3012() {
    let r = check(
        "fn main() {\n\
           var sc chan<- int = make(chan int, 1)\n\
           run := fn(ch <-chan int) {\n\
             var v = <-ch\n\
             _ = v\n\
           }\n\
           run(sc)\n\
           get := fn() <-chan int {\n\
             return sc\n\
           }\n\
           _ = get\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(error_codes(&r), vec!["HYB-E3012", "HYB-E3012"]);
}

// ---------------------------------------------------------------------------
// Issue #623 Repro 10: Method argument (`b.take(sc)`) + struct field
// ---------------------------------------------------------------------------
#[test]
fn repro_10_method_arg_direction_mismatch_rejects_e3012() {
    let r = check(
        "struct box {\n\
           ch <-chan int\n\
         }\n\
         fn (b box) take(rc <-chan int) {\n\
           var v = <-rc\n\
           _ = v\n\
         }\n\
         fn main() {\n\
           var sc chan<- int = make(chan int, 1)\n\
           b := box{}\n\
           b.take(sc)\n\
           h := box{ch: sc}\n\
           _ = h\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(error_codes(&r), vec!["HYB-E3012", "HYB-E3012"]);
}

// ---------------------------------------------------------------------------
// Issue #623 Repro 11: Channel-element direction at send/receive (chan-of-chan)
// ---------------------------------------------------------------------------
#[test]
fn repro_11_chan_of_chan_send_and_recv_direction_mismatch_reject_e3012() {
    let r = check(
        "fn main() {\n\
           outer := make(chan chan<- int, 1)\n\
           var rc <-chan int = make(chan int, 1)\n\
           outer <- rc\n\
           var got <-chan int\n\
           got = <-outer\n\
           _ = got\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(error_codes(&r), vec!["HYB-E3012", "HYB-E3012"]);
}

// ---------------------------------------------------------------------------
// Additional edge cases: slice/map literals, select send, type aliases, append
// ---------------------------------------------------------------------------
#[test]
fn slice_map_lit_and_select_send_direction_mismatch_reject_e3012() {
    let r = check(
        "type recv_int = <-chan int\n\
         fn main() {\n\
           var sc chan<- int = make(chan int, 1)\n\
           s := []recv_int{sc}\n\
           m := map[string]recv_int{\"k\": sc}\n\
           outer := make(chan recv_int, 1)\n\
           select {\n\
           outer <- sc:\n\
             _ = s\n\
           default:\n\
             _ = m\n\
           }\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(
        error_codes(&r),
        vec!["HYB-E3012", "HYB-E3012", "HYB-E3012"]
    );
}

// ---------------------------------------------------------------------------
// Positive controls: bidir -> unidir (both directions) and bidir -> bidir
// ---------------------------------------------------------------------------
#[test]
fn controls_bidir_to_unidir_and_same_dir_accept() {
    let r = check(
        "type recv_int = <-chan int\n\
         struct holder {\n\
           rc <-chan int\n\
           sc chan<- int\n\
         }\n\
         fn take_bidir(c chan int) { _ = c }\n\
         fn take_recv(rc <-chan int) { _ = rc }\n\
         fn take_send(sc chan<- int) { _ = sc }\n\
         fn make_recv(c chan int) <-chan int { return c }\n\
         fn main() {\n\
           c := make(chan int, 1)\n\
           var sc chan<- int = c\n\
           var rc <-chan int = c\n\
           var bc chan int = c\n\
           take_bidir(c)\n\
           take_recv(c)\n\
           take_recv(rc)\n\
           take_send(c)\n\
           take_send(sc)\n\
           h := holder{rc: c, sc: sc}\n\
           outer := make(chan chan<- int, 1)\n\
           outer <- c\n\
           outer <- sc\n\
           outer2 := make(chan recv_int, 1)\n\
           outer2 <- c\n\
           outer2 <- rc\n\
           got := make_recv(bc)\n\
           _ = h\n\
           _ = got\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    assert!(r.diagnostics.is_empty());
}

#[test]
fn generic_struct_method_and_fn_type_args_enforce_e3012() {
    let r = check(
        "struct box[t] {\n\
           val t\n\
         }\n\
         fn (b box[t]) put(v t) {}\n\
         struct pair[a, b] {\n\
           first a\n\
           second b\n\
         }\n\
         fn take_pair[a, b](x a, y b) {}\n\
         fn id[t](x t) t {\n\
           return x\n\
         }\n\
         fn main() {\n\
           sc := make(chan<- int, 1)\n\
           c := make(chan int, 1)\n\
           b := box[<-chan int]{val: sc}\n\
           b.put(sc)\n\
           var b2 box[<-chan int] = box[chan int]{val: c}\n\
           take_pair[<-chan int, int](sc, 1)\n\
           var r <-chan int = id(sc)\n\
           p := pair[<-chan int, int]{first: sc, second: 1}\n\
           _ = b2\n\
           _ = r\n\
           _ = p\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject);
    let e3012_count = r.diagnostics.iter().filter(|d| d.code == "HYB-E3012").count();
    assert_eq!(e3012_count, 6, "expected 6 HYB-E3012 diagnostics, got {:?}", r.diagnostics);
}

#[test]
fn comma_ok_receive_and_map_lookup_enforce_e3012() {
    let r = check(
        "fn main() {\n\
           outer := make(chan chan<- int, 1)\n\
           m := map[int]chan<- int{}\n\
           var got <-chan int\n\
           var ok bool\n\
           got, ok = <-outer\n\
           got2, ok2 := <-outer\n\
           var r2 <-chan int = got2\n\
           got, ok = m[0]\n\
           got3, ok3 := m[0]\n\
           var r3 <-chan int = got3\n\
           _ = got\n\
           _ = ok\n\
           _ = ok2\n\
           _ = r2\n\
           _ = ok3\n\
           _ = r3\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject);
    let e3012_count = r.diagnostics.iter().filter(|d| d.code == "HYB-E3012").count();
    assert_eq!(e3012_count, 4, "expected 4 HYB-E3012 diagnostics, got {:?}", r.diagnostics);
}

#[test]
fn spread_named_args_some_slice_pop_set_and_conversion_enforce_e3012() {
    let r = check(
        "type recv_int = <-chan int\n\
         fn connect(host string, ch <-chan int) {}\n\
         fn main() {\n\
           sc := make(chan<- int, 1)\n\
           s1 := []<-chan int{}\n\
           s2 := []chan<- int{sc}\n\
           s3 := append(s1, s2...)\n\
           connect(ch = sc, host = \"localhost\")\n\
           var o option[<-chan int] = some(sc)\n\
           var sub []<-chan int = s2[0:1]\n\
           var popped <-chan int = s2.pop()\n\
           st := set[<-chan int]{}\n\
           st.insert(sc)\n\
           var alias_r recv_int = recv_int(sc)\n\
           _ = s3\n\
           _ = o\n\
           _ = sub\n\
           _ = popped\n\
           _ = st\n\
           _ = alias_r\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject);
    let e3012_count = r.diagnostics.iter().filter(|d| d.code == "HYB-E3012").count();
    assert_eq!(e3012_count, 7, "expected 7 HYB-E3012 diagnostics, got {:?}", r.diagnostics);
}

#[test]
fn top_level_const_alias_and_inferred_multi_param_generic_enforce_e3012() {
    let r = check(
        "fn id[t](x t) t {\n\
           return x\n\
         }\n\
         const fwd_alias = alias\n\
         const fwd_id = id_alias\n\
         const id_alias = id(sc)\n\
         const sc chan<- int = make(chan int, 1)\n\
         const alias = sc\n\
         const alias2 = alias\n\
         fn same[t](a t, b t) {}\n\
         fn main() {\n\
           c := make(chan int, 1)\n\
           var rc <-chan int = alias2\n\
           var rc2 <-chan int = fwd_alias\n\
           var rc3 <-chan int = fwd_id\n\
           same(rc, sc)\n\
           same(c, sc)\n\
           same(sc, c)\n\
           same[chan<- int](sc, c)\n\
           same(c, c)\n\
           same(sc, sc)\n\
           _ = rc\n\
           _ = rc2\n\
           _ = rc3\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject);
    let e3012_count = r.diagnostics.iter().filter(|d| d.code == "HYB-E3012").count();
    assert_eq!(e3012_count, 6, "expected 6 HYB-E3012 diagnostics, got {:?}", r.diagnostics);

    // Self-referential / mutually recursive consts must terminate cleanly without hanging.
    let _ = check(
        "const cyc_a = cyc_b\n\
         const cyc_b = cyc_a\n\
         fn main() {\n\
           var rc <-chan int = cyc_a\n\
           _ = rc\n\
         }",
    );
}


#[test]
fn typed_array_and_move_wrapped_chan_dir_repro() {
    let r = check(
        "struct holder {\n\
           ch <-chan int\n\
         }\n\
         fn main() {\n\
           var sc chan<- int = make(chan int, 1)\n\
           a1 := [1]<-chan int{sc}\n\
           a2 := [...]<-chan int{sc}\n\
           a3 := [1]holder{{ch: sc}}\n\
           a4 := [1][1]<-chan int{{sc}}\n\
           ch_h := make(chan holder, 1)\n\
           ch_h <- move {ch: sc}\n\
           _ = a1\n\
           _ = a2\n\
           _ = a3\n\
           _ = a4\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject);
    let e3012_count = r.diagnostics.iter().filter(|d| d.code == "HYB-E3012").count();
    assert_eq!(e3012_count, 5, "expected 5 HYB-E3012 diagnostics, got {:?}", r.diagnostics);
}
