//! Integration tests for propagating `TypeExpr` across `Expr::Receive` (`<-ch`
//! and `v, ok := <-ch`), type-aliased `make(chan_alias, n)` / `v, ok := m[k]`,
//! and sub-slice / pointer-to-array expressions in `Checker::ty_of_expr` and
//! `Checker::multi_value_tys`.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> (Verdict, Vec<String>) {
    let ast = parse_file("test.hy", src).expect("test source must parse");
    let res = check_file(&ast);
    let codes = res
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect();
    (res.verdict, codes)
}

#[test]
fn receive_and_aliased_map_comma_ok_bindings_reject_const_overflow_on_reassign() {
    let cases = [
        "fn run(ch chan int8) {\n    v := <-ch\n    v = 300\n    _ = v\n}\nfn main() {}\n",
        "fn run(ch chan int8) {\n    v, ok := <-ch\n    if ok {\n        v = 300\n    }\n    _ = v\n}\nfn main() {}\n",
        "type byte_chan = chan uint8\nconst _anchor = 0\nfn main() {\n    ch := make(byte_chan, 1)\n    v := <-ch\n    v = 300\n    _ = v\n}\n",
        "type byte_chan = chan uint8\nconst _anchor = 0\nfn main() {\n    ch := make(byte_chan, 1)\n    v, ok := <-ch\n    if ok {\n        v = 300\n    }\n    _ = v\n}\n",
        "type byte_map = map[string]uint8\nfn run(m byte_map) {\n    v, ok := m[\"k\"]\n    if ok {\n        v = 300\n    }\n    _ = v\n}\nfn main() {}\n",
        "fn main() {\n    f := fn(ch chan int8) {\n        v := <-ch\n        v = 300\n        _ = v\n    }\n    _ = f\n}\n",
        "fn run(ch <-chan int8) {\n    v, ok := <-ch\n    if ok {\n        v = 300\n    }\n    _ = v\n}\nfn main() {}\n",
    ];
    for src in cases {
        let (verdict, codes) = check(src);
        assert_eq!(verdict, Verdict::Reject, "expected Reject for:\n{src}");
        assert!(
            codes.iter().any(|c| c == "HYB-E6002"),
            "expected HYB-E6002, got {codes:?} for:\n{src}"
        );
    }
}

#[test]
fn receive_and_aliased_map_bindings_reject_invalid_range_bounds_and_send_only_iter() {
    let cases = [
        "fn run(ch chan int8) {\n    v := <-ch\n    for i in v..300 {\n        _ = i\n    }\n}\nfn main() {}\n",
        "fn run(ch chan int8) {\n    v, ok := <-ch\n    if ok {\n        for i in v..300 {\n            _ = i\n        }\n    }\n}\nfn main() {}\n",
        "fn main() {\n    f := fn(ch chan int8) {\n        v, ok := <-ch\n        if ok {\n            for i in v..300 {\n                _ = i\n            }\n        }\n    }\n    _ = f\n}\n",
        "fn run(ch chan float64) {\n    for i in (<-ch)..10 {\n        _ = i\n    }\n}\nfn main() {}\n",
        "fn run(ch chan bool) {\n    v, ok := <-ch\n    if ok {\n        for i in v..10 {\n            _ = i\n        }\n    }\n}\nfn main() {}\n",
        "fn run(outer chan chan<- int) {\n    inner := <-outer\n    for x in inner {\n        _ = x\n    }\n}\nfn main() {}\n",
        "fn run(outer chan chan<- int) {\n    inner, ok := <-outer\n    if ok {\n        for x in inner {\n            _ = x\n        }\n    }\n}\nfn main() {}\n",
        "type send_map = map[string]chan<- int\nfn run(m send_map) {\n    inner, ok := m[\"k\"]\n    if ok {\n        for x in inner {\n            _ = x\n        }\n    }\n}\nfn main() {}\n",
    ];
    for src in cases {
        let (verdict, codes) = check(src);
        assert_eq!(verdict, Verdict::Reject, "expected Reject for:\n{src}");
        assert!(
            codes.iter().any(|c| c == "HYB-E2009"),
            "expected HYB-E2009, got {codes:?} for:\n{src}"
        );
    }
}

#[test]
fn receive_slice_and_aliased_map_bindings_preserve_fn_tuple_arity() {
    let e2002_cases = [
        "fn run(ch chan fn(a int, b int) int int) {\n    cb := <-ch\n    p := cb(3, 9)\n    _ = p\n}\nfn main() {}\n",
        "type fn_chan = chan fn(a int, b int) int int\nconst _anchor = 0\nfn main() {\n    ch := make(fn_chan, 1)\n    cb, ok := <-ch\n    if ok {\n        p := cb(3, 9)\n        _ = p\n    }\n}\n",
        "fn run(ch chan fn(a int, b int) int int) {\n    p := (<-ch)(3, 9)\n    _ = p\n}\nfn main() {}\n",
        "type fn_map = map[string]fn(a int, b int) int int\nconst _anchor = 0\nfn run(m fn_map) {\n    cb, ok := m[\"k\"]\n    if ok {\n        p := cb(3, 9)\n        _ = p\n    }\n}\nfn main() {}\n",
        "fn run(xs []fn(a int, b int) int int) {\n    sub := xs[0:1]\n    p := sub[0](3, 9)\n    _ = p\n}\nfn main() {}\n",
        "fn minmax(a int, b int) (int, int) { return a, b }\nfn main() {\n    arr := [2]fn(a int, b int) int int{minmax, minmax}\n    pa := &arr\n    sub := pa[0:1]\n    p := sub[0](3, 9)\n    _ = p\n}\n",
    ];
    for src in e2002_cases {
        let (verdict, codes) = check(src);
        assert_eq!(verdict, Verdict::Reject, "expected Reject for:\n{src}");
        assert!(
            codes.iter().any(|c| c == "HYB-E2002"),
            "expected HYB-E2002, got {codes:?} for:\n{src}"
        );
    }

    let e1002_src = "fn run(ch chan fn(a int) int) {\n    cb := <-ch\n    a, b := cb(3)\n    _ = a + b\n}\nfn main() {}\n";
    let (verdict, codes) = check(e1002_src);
    assert_eq!(verdict, Verdict::Reject);
    assert!(codes.iter().any(|c| c == "HYB-E1002"), "got {codes:?}");
}

#[test]
fn valid_receive_slice_and_aliased_map_operations_accept() {
    let src = "type byte_chan = chan uint8\ntype byte_map = map[string]uint8\ntype fn_chan = chan fn(a int, b int) int int\nconst _anchor = 0\nfn run(m byte_map, outer chan chan int, xs []fn(a int, b int) int int) int {\n    bch := make(byte_chan, 1)\n    v1 := <-bch\n    v1 = 200\n    v2, ok2 := <-bch\n    if ok2 {\n        v2 = 100\n        for i in v2..120 {\n            _ = i\n        }\n    }\n    v3, ok3 := m[\"k\"]\n    if ok3 {\n        v3 = 255\n    }\n    inner, ok_ch := <-outer\n    if ok_ch {\n        for x in inner {\n            _ = x\n        }\n    }\n    fch := make(fn_chan, 1)\n    cb, ok_fn := <-fch\n    if ok_fn {\n        a, b := cb(1, 2)\n        _ = a + b\n    }\n    sub := xs[0:1]\n    x, y := sub[0](3, 4)\n    return int(v1) + int(v2) + int(v3) + x + y\n}\nfn main() {}\n";
    let (verdict, codes) = check(src);
    assert_eq!(verdict, Verdict::Accept, "expected Accept, got {codes:?}");
}
