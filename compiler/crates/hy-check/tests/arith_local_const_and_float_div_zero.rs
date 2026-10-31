use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    check_file(&parse_file("test.hy", src).expect("parses")).diagnostics.into_iter().map(|d| d.code).collect()
}

#[test]
fn local_const_len_float_and_hex_zero_divisors() {
    for src in [
        "fn main() {\n const zero = 0\n x := 10 / zero\n _ = x\n}\n",
        "fn main() {\n const zero = 0\n x := 10 % zero\n _ = x\n}\n",
        "fn main() {\n const a = 3\n const zero = a - 3\n d := 10\n d /= zero\n _ = d\n}\n",
        "fn main() {\n const empty = \"\"\n x := 10 / len(empty)\n _ = x\n}\n",
        "fn main() {\n const zf = 0.0\n const c = 1.0 / zf\n _ = c\n}\n",
        "fn main() {\n const z = 0\n var a [1 / z]int\n _ = a\n}\n",
        "fn main() {\n f := fn() int { const zero = 0\n return 10 / zero }\n _ = f()\n}\n",
        "fn main() {\n const zero = 0\n f := fn() int { return 10 / zero }\n _ = f()\n}\n",
        "fn main() {\n f := fn() int { const empty = \"\"\n return 10 % len(empty) }\n _ = f()\n}\n",
        "fn main() {\n f := fn() float64 { const zf = 0.0\n return 1.0 / zf }\n _ = f()\n}\n",
        "fn main() { x := 10 / (-len(\"\"))\n _ = x }\n",
        "fn main() { x := 10 / (1 + (-len(\"a\")))\n _ = x }\n",
        "const z = len(\"\")\nfn main() { x := 10 / (-z)\n _ = x }\n",
        "fn main() { x := 10 / i64(len(\"\"))\n _ = x }\n",
        "fn main() { x := 10 % int(len(\"ab\") - 2)\n _ = x }\n",
        "const zf = float64(0)\nconst c = 1.0 / zf\nfn main() { _ = c }\n",
        "const c = 1.0 / (-float64(0))\nfn main() { _ = c }\n",
        "const c = 1.0 / (float64(1) - float64(1))\nfn main() { _ = c }\n",
        "const c = float64(1) / 0.0\nfn main() { _ = c }\n",
        "const c = 1.0 / f32(0x0)\nfn main() { _ = c }\n",
        "fn main() { _ = 1.0 / float32(len(\"\")) }\n",
        "fn main() { _ = 1.0 / f64(1.5 - 1.5) }\n",
    ] {
        let cs = codes(src);
        assert!(!cs.is_empty() && cs.iter().all(|c| c == "HYB-E6001"), "got {cs:?}:\n{src}");
    }
    for (src, lit) in [("const z = 0.0\nconst c = 1.0 / z\nfn main() { _ = c }\n", "0x0p0"), ("const a = 0.0\nconst c = 1.0 / (a - 6.0)\nfn main() { _ = c }\n", "0x1.8p2")] {
        let mut ast = parse_file("test.hy", src).expect("parses");
        if let hy_syntax::Decl::Const(c) = &mut ast.decls[0] {
            if let hy_syntax::Expr::Lit { kind, .. } = &mut c.value {
                *kind = hy_syntax::LitKind::Float(lit.to_string());
            }
        }
        assert_eq!(check_file(&ast).diagnostics.into_iter().map(|d| d.code).collect::<Vec<_>>(), vec!["HYB-E6001"]);
    }
    for src in [
        "fn main() {\n const zero = 0\n _ = zero\n if true { zero := 2\n x := 10 / zero\n _ = x }\n}\n",
        "fn main() {\n const zero = 0\n _ = zero\n f := fn(zero int) int { return 10 / zero }\n _ = f(2)\n}\n",
        "fn main() {\n const zero = 0\n _ = zero\n f := fn() int { zero := 2\n return 10 / zero }\n _ = f()\n}\n",
        "fn float64(x int) float64 { _ = x\n return 2.0 }\nfn main() { _ = 1.0 / float64(0) }\n",
        "fn main() {\n const k = 2\n const s = \"ab\"\n x := 10 / k\n y := 10 / (-len(s))\n z := 1.0 / f64(k)\n _ = x + y\n _ = z\n}\n",
    ] {
        assert!(codes(src).is_empty(), "expected 0 diagnostics for:\n{src}");
    }
}
