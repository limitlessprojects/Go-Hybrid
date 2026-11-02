//! typed_width_chan_bindings_3105.rs — the #2712 width check must see the
//! element width of a value bound by `for v in ch` over a typed channel or by
//! a `select` receive arm (`v := <-c:` / `v, ok := <-c:`) (#3105). Every
//! reject case below passed `hyb check` before and its emitted Go fails
//! `go build` (go1.26.9); every accept case builds.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn has_e1002(r: &hy_check::CheckResult) -> bool {
    r.diagnostics
        .iter()
        .any(|d| d.severity == hy_diag::Severity::Error && d.code == "HYB-E1002")
}

#[test]
fn channel_binding_widths_reject_e1002() {
    let cases = [
        "fn main() {\n    c := make(chan int8, 1)\n    c <- 1\n    var out int64 = 0\n    select {\n        v := <-c: out = v\n    }\n    fmt.println(out)\n}\n",
        "fn main() {\n    c := make(chan int8, 1)\n    c <- 1\n    var out int64 = 0\n    var ok2 bool = false\n    select {\n        v, ok := <-c: out = v\n    }\n    fmt.println(out, ok2)\n}\n",
        "fn main() {\n    c := make(chan int8, 1)\n    c <- 1\n    select {\n        v := <-c: {\n            x := v\n            var y int64 = x\n            fmt.println(y)\n        }\n    }\n}\n",
        "fn main() {\n    c := make(chan int8, 1)\n    c <- 1\n    var y int64 = 0\n    select {\n        v := <-c: y = int64(v) + v\n    }\n    fmt.println(y)\n}\n",
        "fn main() {\n    c := make(chan int8, 1)\n    c <- 1\n    close(c)\n    var t int64 = 0\n    for v in c { t = v }\n    fmt.println(t)\n}\n",
        "fn main() {\n    c := make(chan uint16, 1)\n    c <- 1\n    close(c)\n    for v in c {\n        var y int = v\n        fmt.println(y)\n    }\n}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:
{}", src);
        assert!(has_e1002(&r), "expected HYB-E1002 for:
{}", src);
    }
}

#[test]
fn matching_channel_binding_widths_accept() {
    let cases = [
        "fn main() {\n    c := make(chan int8, 1)\n    c <- 1\n    close(c)\n    var s int8 = 0\n    for v in c { s = s + v }\n    select {\n        default: fmt.println(s)\n    }\n    d := make(chan int8, 1)\n    d <- 2\n    select {\n        v := <-d: {\n            var y int8 = v\n            var z int64 = int64(v)\n            fmt.println(y, z)\n        }\n    }\n}\n",
        "fn drain(c <-chan int8) int8 {\n    var t int8 = 0\n    for v in c { t = t + v }\n    return t\n}\nfn main() {\n    c := make(chan int8, 1)\n    c <- 1\n    close(c)\n    fmt.println(drain(c))\n    d := make(chan int64, 1)\n    d <- 2\n    var y int64 = 0\n    select {\n        v, ok := <-d: if ok { y = v }\n    }\n    fmt.println(y)\n}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "expected acceptance for:
{}
{:?}", src, r.diagnostics);
    }
}
