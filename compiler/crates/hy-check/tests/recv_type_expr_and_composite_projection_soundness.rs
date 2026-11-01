//! Regression tests for `recv.rs` (`HYB-E2008` / `HYB-E3005`) across expression
//! `TypeExpr` array lengths, composite field/option/slice projections in
//! `recv_chan_type_expr`, and `if`-init / `match`-guard bindings.

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|d| panic!("must parse: {d:?}"));
    check_file(&ast)
}

fn recv_errors(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error && matches!(d.code.as_str(), "HYB-E2008" | "HYB-E3005"))
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect()
}

#[test]
fn expr_type_array_lengths_check_receive_operands() {
    for (label, code, needle, src) in [
        (
            "non-channel receive in array composite literal type head",
            "HYB-E2008",
            "cannot receive from `x`",
            "fn main() {\n x := 1\n xs := [<-x]int{}\n fmt.println(\"{len(xs)}\")\n}\n",
        ),
        (
            "send-only channel receive in array composite literal type head",
            "HYB-E2008",
            "channel is send-only",
            "fn f(ch chan<- int) {\n xs := [<-ch]int{}\n fmt.println(\"{len(xs)}\")\n}\nfn main() {}\n",
        ),
        (
            "send-only channel receive in map literal value array type",
            "HYB-E2008",
            "channel is send-only",
            "fn f(ch chan<- int) {\n m := map[string][<-ch]int{}\n fmt.println(\"{len(m)}\")\n}\nfn main() {}\n",
        ),
        (
            "send-only channel receive in generic struct literal type argument",
            "HYB-E2008",
            "channel is send-only",
            "struct box[t] { v t }\nfn f(ch chan<- int) {\n b := box[[<-ch]int]{}\n fmt.println(\"{len(b.v)}\")\n}\nfn main() {}\n",
        ),
        (
            "send-only channel receive in closure parameter array type",
            "HYB-E2008",
            "channel is send-only",
            "fn f(ch chan<- int) {\n g := fn(a [<-ch]int) int { return len(a) }\n _ = g\n}\nfn main() {}\n",
        ),
        (
            "unproven receive in array composite literal type head",
            "HYB-E3005",
            "cannot prove `mystery(...)` is a channel",
            "fn main() {\n xs := [<-mystery()]int{}\n fmt.println(\"{len(xs)}\")\n}\n",
        ),
    ] {
        let r = check(src);
        let errs = recv_errors(&r);
        assert!(
            errs.iter().any(|e| e.starts_with(code) && e.contains(needle)),
            "{label} must emit {code} mentioning `{needle}` for:\n{src}\ngot: {errs:?}"
        );
    }
}

#[test]
fn composite_field_and_method_projections_preserve_send_only_channel_dir() {
    for (label, src) in [
        (
            "field projection on receive operand `<-(<-outer).ch`",
            "struct holder { ch chan<- int }\nfn f(outer chan holder) {\n var a [<-(<-outer).ch]int\n fmt.println(\"{len(a)}\")\n}\nfn main() {}\n",
        ),
        (
            "local bound from `(<-outer).ch` then received in array length",
            "struct holder { ch chan<- int }\nfn f(outer chan holder) {\n sub := (<-outer).ch\n var a [<-sub]int\n fmt.println(\"{len(a)}\")\n}\nfn main() {}\n",
        ),
        (
            "field projection on `append(hs, h)[0].ch`",
            "struct holder { ch chan<- int }\nfn f(hs []holder, h holder) {\n var a [<-append(hs, h)[0].ch]int\n fmt.println(\"{len(a)}\")\n}\nfn main() {}\n",
        ),
        (
            "field projection on slice expression `hs[0..1][0].ch`",
            "struct holder { ch chan<- int }\nfn f(hs []holder) {\n var a [<-hs[0..1][0].ch]int\n fmt.println(\"{len(a)}\")\n}\nfn main() {}\n",
        ),
        (
            "option `.unwrap()` on composite `(<-outer).unwrap()`",
            "fn f(outer chan option[chan<- int]) {\n var a [<-(<-outer).unwrap()]int\n fmt.println(\"{len(a)}\")\n}\nfn main() {}\n",
        ),
        (
            "slice `.pop()` on composite `(<-outer).pop()`",
            "fn f(outer chan []chan<- int) {\n var a [<-(<-outer).pop()]int\n fmt.println(\"{len(a)}\")\n}\nfn main() {}\n",
        ),
    ] {
        let r = check(src);
        let errs = recv_errors(&r);
        assert!(
            errs.iter()
                .any(|e| e.starts_with("HYB-E2008") && e.contains("channel is send-only")),
            "{label} must emit HYB-E2008 send-only diagnostic for:\n{src}\ngot: {errs:?}"
        );
    }

    // Shadowed `append` parameter returning bidirectional `[]chan int` must NOT
    // inherit the first argument's `[]chan<- int` send-only direction.
    let shadowed = "fn f(append fn(a []chan<- int, b chan<- int) []chan int, chs []chan<- int, ch chan<- int) {\n var a [<-append(chs, ch)[0]]int\n fmt.println(\"{len(a)}\")\n}\nfn main() {}\n";
    let r = check(shadowed);
    let errs = recv_errors(&r);
    assert!(
        errs.is_empty(),
        "shadowed `append` parameter returning `[]chan int` must not emit HYB-E2008/E3005, got: {errs:?}"
    );

    // Valid bidirectional `chan int` unwrapped from composite `(<-outer).unwrap()` must not false-reject with HYB-E3005.
    let valid_unwrap = "fn f(outer chan option[chan int]) int {\n return <-(<-outer).unwrap()\n}\nfn main() {}\n";
    let r = check(valid_unwrap);
    let errs = recv_errors(&r);
    assert!(
        errs.is_empty(),
        "valid `(<-outer).unwrap()` returning `chan int` must not emit HYB-E2008/E3005, got: {errs:?}"
    );

    // Non-channel `int` unwrapped from composite `(<-outer).unwrap()` must emit HYB-E2008 (not HYB-E3005).
    let non_chan_unwrap = "fn f(outer chan option[int]) int {\n return <-(<-outer).unwrap()\n}\nfn main() {}\n";
    let r = check(non_chan_unwrap);
    let errs = recv_errors(&r);
    assert!(
        errs.iter()
            .any(|e| e.starts_with("HYB-E2008") && e.contains("a receive source must be a channel")),
        "non-channel `(<-outer).unwrap()` must emit HYB-E2008, got: {errs:?}"
    );
}

#[test]
fn if_init_and_match_guard_bindings_classify_receive_operands() {
    for (label, needle, src) in [
        (
            "if-init bound from composite `<-outer` used in condition array type",
            "channel is send-only",
            "fn f(outer chan chan<- int) {\n if sub := <-outer; len([<-sub]int{}) == 0 {\n fmt.println(\"ok\")\n }\n}\nfn main() {}\n",
        ),
        (
            "match `some(ch)` on composite `<-outer_opt` used in guard array type",
            "channel is send-only",
            "fn f(outer_opt chan option[chan<- int]) {\n match <-outer_opt {\n some(ch) if len([<-ch]int{}) == 0 => { fmt.println(\"ok\") }\n _ => {}\n }\n}\nfn main() {}\n",
        ),
        (
            "match enum variant `count(ch)` with int payload shadowing outer `ch chan int` in guard",
            "cannot receive from `ch`",
            "enum event {\n count(int)\n idle\n}\nfn f(ev event, ch chan int) {\n match ev {\n count(ch) if (<-ch) > 0 => { fmt.println(\"ok\") }\n _ => {}\n }\n}\nfn main() {}\n",
        ),
        (
            "match enum variant `out(ch)` with `chan<- int` payload in guard array type",
            "channel is send-only",
            "enum event {\n out(chan<- int)\n idle\n}\nfn f(ev event) {\n match ev {\n out(ch) if len([<-ch]int{}) == 0 => { fmt.println(\"ok\") }\n _ => {}\n }\n}\nfn main() {}\n",
        ),
    ] {
        let r = check(src);
        let errs = recv_errors(&r);
        assert!(
            errs.iter().any(|e| e.starts_with("HYB-E2008") && e.contains(needle)),
            "{label} must emit HYB-E2008 mentioning `{needle}` for:\n{src}\ngot: {errs:?}"
        );
    }

    // Valid bidirectional `chan int` payload in enum variant match guard must not false-reject with HYB-E3005.
    let valid_enum_chan = "enum event {\n ready(chan int)\n idle\n}\nfn f(ev event) {\n match ev {\n ready(ch) if (<-ch) > 0 => { fmt.println(\"ok\") }\n _ => {}\n }\n}\nfn main() {}\n";
    let r = check(valid_enum_chan);
    let errs = recv_errors(&r);
    assert!(
        errs.is_empty(),
        "valid bidirectional `chan int` enum payload in match guard must not emit HYB-E2008/E3005, got: {errs:?}"
    );
}
