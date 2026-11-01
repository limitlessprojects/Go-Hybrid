//! rune_literal_binding_width.rs — `r := 'x'` binds a Go `rune` (`int32`),
//! not an `int`. A rune-literal binding mixed with an `int` in arithmetic,
//! comparison, assignment, a call argument, or a var initializer emits Go
//! that fails `go build` (mismatched types rune and int), so `hyb check`
//! must reject it with HYB-E1002 (GUARANTEES row 13, BUILDABLE).

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
fn rune_literal_binding_mixed_with_int_rejects_e1002() {
    let cases = [
        // binary arithmetic / bitwise / comparison, both operand orders
        "fn main() {\n    i := 1\n    r := 'x'\n    z := r + i\n    fmt.println(z)\n}\n",
        "fn main() {\n    i := 1\n    r := 'x'\n    z := i - r\n    fmt.println(z)\n}\n",
        "fn main() {\n    i := 1\n    r := 'x'\n    z := r % i\n    fmt.println(z)\n}\n",
        "fn main() {\n    i := 1\n    r := 'x'\n    z := i & r\n    fmt.println(z)\n}\n",
        "fn main() {\n    i := 1\n    r := 'x'\n    fmt.println(r == i)\n}\n",
        "fn main() {\n    i := 1\n    r := 'x'\n    fmt.println(i < r)\n}\n",
        // parenthesised initializer
        "fn main() {\n    i := 1\n    r := ('x')\n    fmt.println(r + i)\n}\n",
        // var without ascription
        "fn main() {\n    i := 1\n    var r = 'x'\n    fmt.println(r + i)\n}\n",
        // assignment and var initializer into int
        "fn main() {\n    var i int = 1\n    r := 'x'\n    i = r\n    fmt.println(i)\n}\n",
        "fn main() {\n    r := 'x'\n    var n int = r\n    fmt.println(n)\n}\n",
        // call argument
        "fn take(n int) { fmt.println(n) }\nfn main() {\n    r := 'x'\n    take(r)\n}\n",
        // other widths
        "fn main() {\n    var b u8 = 1\n    r := 'x'\n    fmt.println(r + b)\n}\n",
        "fn main() {\n    var w i64 = 1\n    r := 'x'\n    fmt.println(w + r)\n}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}", src);
        assert!(has_e1002(&r), "expected HYB-E1002 for:\n{}", src);
    }
}

#[test]
fn rune_literal_binding_same_type_still_accepts() {
    let cases = [
        "fn main() {\n    r := 'x'\n    z := r + 1\n    fmt.println(z)\n}\n",
        "fn main() {\n    r := 'x'\n    z := r + 'y'\n    fmt.println(z)\n}\n",
        "fn main() {\n    r := 'x'\n    var c rune = r\n    fmt.println(c)\n}\n",
        "fn main() {\n    r := 'x'\n    var c i32 = r\n    fmt.println(c + r)\n}\n",
        "fn main() {\n    r := 'x'\n    var c int32 = 2\n    fmt.println(c * r)\n}\n",
        "fn main() {\n    r := 'x'\n    n := int(r)\n    i := 1\n    fmt.println(n + i)\n}\n",
        "fn take(c rune) { fmt.println(c) }\nfn main() {\n    r := 'x'\n    take(r)\n}\n",
        "fn main() {\n    r := 'x'\n    fmt.println(r == 'x')\n}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "expected acceptance for:\n{}\n{:?}", src, r.diagnostics);
    }
}
