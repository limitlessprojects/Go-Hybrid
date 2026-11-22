//! untyped_const_expr_binding_width.rs — a binding initialised from an
//! untyped constant EXPRESSION (`x := 1 + 2`, `x := 1 << 3`, `x := 'a' + 1`)
//! or from a named untyped constant (`const c = 5; x := c`) takes Go's
//! default type (`int`, `rune`, `float64`) exactly like a bare literal does
//! (#1031 / #2984). Mixing that binding with a different concrete numeric
//! type emits Go that fails `go build`, so `hyb check` must reject it with
//! HYB-E1002 (GUARANTEES row 13, BUILDABLE).

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
fn untyped_const_expression_binding_rejects_other_widths() {
    let cases = [
        // arithmetic / bitwise / shift of untyped integer literals
        "fn main() {\n    x := 1 + 2\n    var y int64 = x\n    fmt.println(y)\n}\n",
        "fn main() {\n    x := 7 % 3\n    var y int64 = x\n    fmt.println(y)\n}\n",
        "fn main() {\n    x := 3 | 4\n    var y int64 = x\n    fmt.println(y)\n}\n",
        "fn main() {\n    x := 1 << 3\n    var y int64 = x\n    fmt.println(y)\n}\n",
        "fn main() {\n    x := (1 + 2) * 3\n    var y u16 = x\n    fmt.println(y)\n}\n",
        // float and rune constant expressions
        "fn main() {\n    x := 1.5 * 2\n    var y float32 = x\n    fmt.println(y)\n}\n",
        "fn main() {\n    x := 1 + 2.5\n    var y float32 = x\n    fmt.println(y)\n}\n",
        "fn main() {\n    x := 'a' + 1\n    var y int = x\n    fmt.println(y)\n}\n",
        // mixed-width arithmetic, assignment and call argument
        "fn main() {\n    x := 1 + 2\n    y := uint8(1) + x\n    fmt.println(y)\n}\n",
        "fn main() {\n    var y int8 = 1\n    x := 2 * 3\n    y = x\n    fmt.println(y)\n}\n",
        "fn take(a int64) int64 { return a }\nfn main() {\n    x := 1 + 2\n    fmt.println(take(x))\n}\n",
        // `var` without ascription and `let`
        "fn main() {\n    var x = 1 + 2\n    var y int64 = x\n    fmt.println(y)\n}\n",
        "fn main() {\n    let x = 1 + 2\n    var y int64 = x\n    fmt.println(y)\n}\n",
        // named untyped constants, local and file-level
        "fn main() {\n    const c = 5\n    x := c\n    var y int64 = x\n    fmt.println(y)\n}\n",
        "fn main() {\n    const c = 5\n    x := c + 1\n    y := uint8(1) + x\n    fmt.println(y)\n}\n",
        "fn main() {\n    const c = 5.5\n    x := c\n    var y float32 = x\n    fmt.println(y)\n}\n",
        "fn main() {\n    const c = 'a'\n    x := c\n    var y int = x\n    fmt.println(y)\n}\n",
        "const base = 10\nfn main() {\n    x := base * 2\n    var y int64 = x\n    fmt.println(y)\n}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected rejection for:\n{}", src);
        assert!(has_e1002(&r), "expected HYB-E1002 for:\n{}", src);
    }
}

#[test]
fn untyped_const_expression_binding_same_type_still_accepts() {
    let cases = [
        "fn main() {\n    x := 1 + 2\n    var y int = x\n    z := x + 1\n    fmt.println(y, z)\n}\n",
        "fn main() {\n    x := 1 << 3\n    fmt.println(float64(x), int64(x))\n}\n",
        "fn main() {\n    x := 1.5 * 2\n    var y float64 = x\n    fmt.println(y)\n}\n",
        "fn main() {\n    x := 'a' + 1\n    var y rune = x\n    var z i32 = x\n    fmt.println(y, z)\n}\n",
        // a named constant itself stays untyped: assignable to any width
        "fn main() {\n    const c = 5\n    var y int64 = c\n    var z u8 = c\n    fmt.println(y, z)\n}\n",
        // typed constants are not pinned to `int`
        "fn main() {\n    const c = int64(5)\n    x := c\n    var y int64 = x\n    fmt.println(y)\n}\n",
        "fn main() {\n    const c int64 = 5\n    x := c + 1\n    var y int64 = x\n    fmt.println(y)\n}\n",
        // a parameter / inner binding shadows a file constant
        "const base = 10\nfn f(base int64) int64 {\n    x := base + 1\n    var y int64 = x\n    return y\n}\nfn main() {\n    fmt.println(f(1))\n}\n",
        "fn main() {\n    const c = 5\n    {\n        c := int64(1)\n        x := c\n        var y int64 = x\n        fmt.println(y)\n    }\n    fmt.println(c)\n}\n",
        // typed operand wins: `int64(1) + 2` is int64
        "fn main() {\n    x := int64(1) + 2\n    var y int64 = x\n    fmt.println(y)\n}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "expected acceptance for:\n{}\n{:?}", src, r.diagnostics);
    }
}
