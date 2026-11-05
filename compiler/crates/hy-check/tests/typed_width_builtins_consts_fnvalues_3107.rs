//! typed_width_builtins_consts_fnvalues_3107.rs — the #2712 width check must
//! see the `int` result of `len` / `cap` / `copy` (including `len` of a const
//! string or an array, which is a constant OF TYPE int), the declared width of
//! a typed file-level const, and the result type of a local bound to a named
//! function or a method value (#3107). Every reject case below passed
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
fn builtin_const_fn_value_widths_reject_e1002() {
    let cases = [
        "fn main() {\n    xs := []int{1}\n    var n i32 = len(xs)\n    fmt.println(n)\n}\n",
        "fn main() {\n    xs := []int{1}\n    n := len(xs)\n    var y i32 = n\n    fmt.println(y)\n}\n",
        "fn main() {\n    var a [3]int\n    var n i32 = len(a)\n    var m i64 = cap(a)\n    fmt.println(n, m)\n}\n",
        "const s = \"abc\"\nfn main() {\n    const t = \"hello\"\n    var n i32 = len(s)\n    var m u8 = len(t)\n    var q i64 = len(\"xyz\")\n    fmt.println(n, m, q)\n}\n",
        "fn main() {\n    xs := make([]int, 1, 4)\n    var n i64 = cap(xs)\n    fmt.println(n)\n}\n",
        "fn main() {\n    a := []int{1}\n    b := []int{2}\n    var n u8 = copy(a, b)\n    fmt.println(n)\n}\n",
        "const k i8 = 3\nfn main() {\n    var y i64 = k\n    fmt.println(y)\n}\n",
        "fn get() i8 { return 1 }\nfn main() {\n    f := get\n    var y i64 = f()\n    fmt.println(y)\n}\n",
        "struct c { n i8 }\nfn (x c) get() i8 { return x.n }\nfn main() {\n    a := c{n: 1}\n    f := a.get\n    var y i64 = f()\n    fmt.println(y)\n}\n",
        "const k i8 = 3\nfn take(v i64) { fmt.println(v) }\nfn main() {\n    take(k)\n}\n",
        "fn main() {\n    s := \"abc\"\n    var n i32 = len(s)\n    fmt.println(n)\n}\n",
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
fn matching_builtin_const_fn_value_widths_accept() {
    let cases = [
        "const k i8 = 3\nfn get() i8 { return 1 }\nfn main() {\n    xs := []int{1}\n    var n int = len(xs)\n    var y i8 = k\n    f := get\n    var z i8 = f()\n    var u i64 = 3\n    fmt.println(n, y, z, u)\n}\n",
        "fn main() {\n    m := map[string]int{\"a\": 1}\n    ch := make(chan int, 3)\n    var n int = len(m)\n    var c int = cap(ch)\n    n2 := len(m) + 1\n    var x int = n2\n    fmt.println(n, c, x)\n}\n",
        "struct c { n i8 }\nfn (x c) get() i8 { return x.n }\nfn (x c) two() (i8, i64) { return x.n, 2 }\nfn main() {\n    a := c{n: 1}\n    f := a.get\n    var y i8 = f()\n    var z int = int(f())\n    p, q := a.two()\n    fmt.println(y, z, p, q)\n}\n",
        "const k i8 = 3\nfn get() i8 { return 1 }\nfn main() {\n    k := 5\n    var y int = k\n    get := fn() i64 { return 2 }\n    var z i64 = get()\n    fmt.println(y, z)\n}\n",
        "const k = 3\nconst f f32 = 1.5\nfn take(v i64) { fmt.println(v) }\nfn takef(v f32) { fmt.println(v) }\nfn main() {\n    var y i8 = k\n    take(k)\n    takef(f)\n    fmt.println(y)\n}\n",
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
