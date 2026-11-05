//! typed_width_generics_slices_3111.rs — the #2712 width check must see the
//! typed results of generic fn calls (type arguments inferred from typed
//! arguments, explicit, or the default type of an all-untyped-literal
//! argument), generic struct method results and field reads with the
//! instance's type arguments substituted, and the element type of a
//! slice-expression binding (#3111). Every reject case below passed
//! `hyb check` before and its emitted Go fails `go build` (go1.26.9); every
//! accept case builds and runs.

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
fn generic_and_slice_expr_widths_reject_e1002() {
    let cases = [
        r#"fn id[t any](v t) t { return v }
fn main() {
    var a int8 = 1
    var y int64 = id(a)
    fmt.println(y)
}
"#,
        r#"fn id[t any](v t) t { return v }
fn main() {
    var a int8 = 1
    var y int64 = id[int8](a)
    fmt.println(y)
}
"#,
        r#"fn first[t any](xs []t) t { return xs[0] }
fn main() {
    xs := []int8{1}
    var y int64 = first(xs)
    fmt.println(y)
}
"#,
        r#"fn id[t any](v t) t { return v }
fn take(v int64) int64 { return v }
fn main() {
    var a int32 = 1
    fmt.println(take(id(a)))
}
"#,
        r#"fn id[t any](v t) t { return v }
fn main() {
    var e int64 = id(3)
    fmt.println(e)
}
"#,
        r#"struct box[t any] { v t }
fn (b box[t]) get() t { return b.v }
fn main() {
    b := box[int8]{v: 1}
    var y int64 = b.get()
    fmt.println(y)
}
"#,
        r#"struct box[t any] { v t }
fn (b box[t]) get() t { return b.v }
fn widen() int64 {
    b := box[int8]{v: 1}
    return b.get()
}
fn main() { fmt.println(widen()) }
"#,
        r#"struct box[t any] { v t }
fn main() {
    b := box[int8]{v: 1}
    var y int64 = b.v
    fmt.println(y)
}
"#,
        r#"struct box[t any] { v t }
fn main() {
    b := box[uint16]{v: 1}
    x := b.v
    var y int = x
    fmt.println(y)
}
"#,
        r#"fn main() {
    arr := [2]int8{1, 2}
    s := arr[0:1]
    var y int64 = s[0]
    fmt.println(y)
}
"#,
        r#"fn main() {
    var xs []int8 = []int8{1, 2}
    s := xs[1:]
    var y int64 = s[0]
    fmt.println(y)
}
"#,
    ];
    for src in cases {
        let r = check(src);
        assert!(has_e1002(&r), "expected HYB-E1002 for:
{}
{:?}", src, r.diagnostics);
    }
}

#[test]
fn generic_and_slice_expr_widths_accept() {
    let cases = [
        r#"struct box[t any] { v t }
fn (b box[t]) get() t { return b.v }
fn id[t any](v t) t { return v }
fn first[t any](xs []t) t { return xs[0] }
fn main() {
    b := box[int8]{v: 1}
    var a int8 = b.get()
    var c int8 = b.v
    var d int8 = id(a)
    var e int64 = id[int64](3)
    xs := []int8{1, 2}
    s := xs[1:]
    var f int8 = s[0]
    var g int8 = first(xs)
    var h int64 = int64(id(a))
    fmt.println(a, c, d, e, f, g, h)
}
"#,
        r#"struct box[t any] { v t }
fn (b box[u]) get() u { return b.v }
struct pr[k comparable, v any] { a k
    b v }
fn (p pr[k, v]) second() v { return p.b }
fn pair[t any](a t, b t) t { return a }
fn mk[t any](v t) []t { return []t{v} }
fn main() {
    var a int8 = 1
    var x int8 = pair(a, 3)
    var y int8 = pair(3, a)
    var f float64 = pair(1, 2.5)
    b := box[int16]{v: 2}
    var z int16 = b.get()
    p := pr[string, uint8]{a: "k", b: 7}
    var w uint8 = p.second()
    var w2 uint8 = p.b
    m := map[string]int32{"a": 1}
    var q int32 = m["a"]
    arr := [3]int64{1, 2, 3}
    s := arr[1:]
    var r int64 = s[1]
    xs := mk(a)
    var g int8 = xs[0]
    fmt.println(x, y, f, z, w, w2, q, r, g)
}
"#,
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "expected acceptance for:
{}
{:?}",
            src,
            r.diagnostics
        );
    }
}
