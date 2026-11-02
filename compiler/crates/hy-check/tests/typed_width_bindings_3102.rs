//! typed_width_bindings_3102.rs — the #2712 width check must see the declared
//! width of a value that comes from a `for … in` binding over a typed slice,
//! array or map, from a method call, from a call through a local bound to a
//! one-result fn literal, or from a channel receive (#3102). Every reject case
//! below passed `hyb check` before and its emitted Go fails `go build`
//! (go1.26.9); every accept case builds.

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
fn typed_width_from_bindings_and_calls_rejects_e1002() {
    let cases = [
        // for-in element over a typed slice / array / map
        "fn main() {\n    xs := []int8{1, 2}\n    for x in xs {\n        var y int64 = x\n        fmt.println(y)\n    }\n}\n",
        "fn main() {\n    xs := [3]int16{1, 2, 3}\n    for i, x in xs {\n        var y int = x\n        fmt.println(i, y)\n    }\n}\n",
        "fn main() {\n    xs := []float32{1.5}\n    for x in xs {\n        var y float64 = x\n        fmt.println(y)\n    }\n}\n",
        "fn main() {\n    m := map[string]int8{\"a\": 1}\n    for k, v in m {\n        var y int64 = v\n        fmt.println(k, y)\n    }\n}\n",
        "fn main() {\n    m := map[int8]string{1: \"a\"}\n    for k, v in m {\n        var y int64 = k\n        fmt.println(y, v)\n    }\n}\n",
        "fn take(y int64) { fmt.println(y) }\nfn main() {\n    xs := []int8{1}\n    for x in xs { take(x) }\n}\n",
        "fn main() {\n    xs := []int8{1}\n    var t int64 = 0\n    for x in xs { t = t + x }\n    fmt.println(t)\n}\n",
        // typed declaration and typed parameter as the iterable
        "fn main() {\n    var xs []i8 = []i8{1}\n    for x in xs {\n        var y int = x\n        fmt.println(y)\n    }\n}\n",
        "fn sum(xs []u16) int {\n    var t int = 0\n    for x in xs { t = t + x }\n    return t\n}\nfn main() { fmt.println(sum([]u16{1})) }\n",
        // method call result
        "struct p { x int }\nfn (q p) g() int8 { return 1 }\nfn main() {\n    a := p{x: 1}\n    var y int64 = a.g()\n    fmt.println(y)\n}\n",
        "struct p { x int }\nfn (q *p) g() int8 { return 1 }\nfn main() {\n    a := &p{x: 1}\n    x := a.g()\n    var y int64 = x\n    fmt.println(y)\n}\n",
        // call through a local bound to a one-result fn literal
        "fn main() {\n    f := fn() int8 { return 1 }\n    var y int64 = f()\n    fmt.println(y)\n}\n",
        // channel receive
        "fn main() {\n    c := make(chan int8, 1)\n    c <- 1\n    var y int64 = <-c\n    fmt.println(y)\n}\n",
        "fn main() {\n    c := make(chan u16, 1)\n    c <- 1\n    x := <-c\n    var y int = x\n    fmt.println(y)\n}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected rejection for:\n{}",
            src
        );
        assert!(has_e1002(&r), "expected HYB-E1002 for:\n{}", src);
    }
}

#[test]
fn matching_widths_and_unknown_sources_still_accept() {
    let cases = [
        // matching widths through every new path
        "struct p { x int }\nfn (q p) g() int8 { return 1 }\nfn main() {\n    xs := []int8{1, 2}\n    var t int8 = 0\n    for x in xs { t = t + x }\n    m := map[int8]float32{1: 1.5}\n    for k, v in m {\n        var a int8 = k\n        var b float32 = v\n        fmt.println(a, b)\n    }\n    a := p{x: 1}\n    var g int8 = a.g()\n    f := fn() int8 { return 1 }\n    var h int8 = f()\n    c := make(chan int8, 1)\n    c <- 1\n    var r int8 = <-c\n    var big int64 = int64(t) + 1\n    fmt.println(t, g, h, r, big)\n}\n",
        // the index binding of a slice range is an int
        "fn main() {\n    xs := []int8{1}\n    for i, x in xs {\n        var n int = i\n        fmt.println(n, x)\n    }\n}\n",
        // an inner rebinding without type text does not inherit the outer one
        "fn ints() []int { return []int{1} }\nfn main() {\n    xs := []int8{1}\n    if len(xs) > 0 {\n        xs := ints()\n        for x in xs {\n            var y int = x\n            fmt.println(y)\n        }\n    }\n}\n",
        // untyped constants stay assignable from every path's matching side
        "fn main() {\n    xs := []int64{1}\n    for x in xs {\n        y := x + 1\n        var z int64 = y\n        fmt.println(z)\n    }\n}\n",
        // an explicit conversion is fine
        "fn main() {\n    xs := []int8{1}\n    for x in xs {\n        var y int64 = int64(x)\n        fmt.println(y)\n    }\n}\n",
        // a method returning a struct is not a numeric scalar
        "struct p { x int }\nfn (q p) me() p { return q }\nfn main() {\n    a := p{x: 1}\n    b := a.me()\n    var n int = b.x\n    fmt.println(n)\n}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "expected pass for:\n{}\n{:?}",
            src,
            r.diagnostics
                .iter()
                .map(|d| format!("{} {}", d.code, d.message))
                .collect::<Vec<_>>()
        );
    }
}
