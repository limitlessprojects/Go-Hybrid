use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    check_file(&parse_file("test.hy", src).expect("test program should parse"))
}

#[test]
fn int_const_overflow_in_any_slots_rejects_e6002() {
    for src in [
        // 1. fmt.println(1 << 64)
        "fn main() {\n fmt.println(1 << 64)\n}",
        // 2. fmt.println(big * 4)
        "const big = 1 << 62\nfn main() {\n fmt.println(big * 4)\n}",
        // 3. fmt.println(big + big + big)
        "const big = 1 << 62\nfn main() {\n fmt.println(big + big + big)\n}",
        // 4. fmt.println(18446744073709551615)
        "fn main() {\n fmt.println(18446744073709551615)\n}",
        // 5. var a any = big * 4
        "const big = 1 << 62\nfn main() {\n var a any = big * 4\n _ = a\n}",
        // 6. s := []any{big * 4}
        "const big = 1 << 62\nfn main() {\n s := []any{big * 4}\n _ = s\n}",
        // 7. fmt.printf with overflowing argument
        "fn main() {\n fmt.printf(\"%v\", 1 << 64)\n}",
    ] {
        let d = check_file(&parse_file("t.hy", src).unwrap()).diagnostics;
        assert!(d.iter().any(|x| x.code == "HYB-E6002"), "{src}: {d:?}");
    }
}

#[test]
fn valid_int_const_in_any_slots_accepted() {
    for src in [
        "fn main() {\n fmt.println(42)\n}",
        "const small = 100\nfn main() {\n fmt.println(small * 2)\n}",
        "fn main() {\n var a any = 10\n _ = a\n}",
        "fn main() {\n s := []any{1, 2, 3}\n _ = s\n}",
    ] {
        let d = check_file(&parse_file("t.hy", src).unwrap()).diagnostics;
        assert!(!d.iter().any(|x| x.code == "HYB-E6002"), "{src}: {d:?}");
    }
}

#[test]
fn untyped_overflow_in_any_decls_assigns_and_composites_rejected_e6002() {
    for src in [
        "import \"fmt\"\ntype item = any\nfn main() {\n    var a item = 18446744073709551615\n    fmt.println(a)\n}\n",
        "import \"fmt\"\nfn main() {\n    var a any = 1\n    a = 1 << 64\n    fmt.println(a)\n}\n",
        "import \"fmt\"\nfn main() {\n    m := map[string]any{\"k\": 1 << 64}\n    fmt.println(m)\n}\n",
        "import \"fmt\"\nstruct box {\n    v any\n}\nfn main() {\n    b := box{v: 1 << 64}\n    fmt.println(b)\n}\n",
        "import \"fmt\"\nfn main() {\n    var xs []any\n    xs = append(xs, 1 << 64)\n    fmt.println(xs)\n}\n",
        "import \"fmt\"\nfn f() any {\n    return 1 << 64\n}\nfn main() {\n    fmt.println(f())\n}\n",
        "import \"fmt\"\nfn take(x any) {\n    fmt.println(x)\n}\nfn main() {\n    take(1 << 64)\n}\n",
        "import \"fmt\"\nfn main() {\n    var a any = 1e400\n    fmt.println(a)\n}\n",
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected reject for:\n{src}");
        assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E6002"), "{src}: {:?}", r.diagnostics);
    }
}

#[test]
fn untyped_overflow_in_fmt_print_panic_and_any_conv_rejected() {
    for src in [
        "import \"fmt\"\nfn main() {\n    s := fmt.sprintf(\"%v\", 1 << 64)\n    fmt.println(s)\n}\n",
        "import \"fmt\"\nfn main() {\n    err := fmt.errorf(\"%v\", 1 << 64)\n    fmt.println(err)\n}\n",
        "fn main() {\n    println(1 << 64)\n}\n",
        "fn main() {\n    print(1 << 64)\n}\n",
        "fn main() {\n    panic(1 << 64)\n}\n",
        "import \"fmt\"\nfn main() {\n    s := \"val={fmt.sprint(1 << 64)}\"\n    fmt.println(s)\n}\n",
        "import \"fmt\"\nfn main() {\n    x := any(1 << 64)\n    fmt.println(x)\n}\n",
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected reject for:\n{src}");
        assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E6002"), "{src}: {:?}", r.diagnostics);
    }
}

#[test]
fn option_some_wrapped_overflow_rejected_and_typed_or_shadowed_accepted() {
    for src in [
        "import \"fmt\"\nfn main() {\n    var o option[u8] = some(300)\n    fmt.println(o.is_some())\n}\n",
        "import \"fmt\"\nfn main() {\n    var o option[any] = some(1 << 64)\n    fmt.println(o.is_some())\n}\n",
        "import \"fmt\"\nfn main() {\n    o := some(1 << 64)\n    fmt.println(o.is_some())\n}\n",
        "import \"fmt\"\nfn main() {\n    var o option[u8] = none\n    o = some(300)\n    fmt.println(o.is_some())\n}\n",
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected reject for:\n{src}");
        assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E6002"), "{src}: {:?}", r.diagnostics);
    }

    for src in [
        "import \"fmt\"\nfn main() {\n    var a any = u64(18446744073709551615)\n    fmt.println(a)\n}\n",
        "import \"fmt\"\nconst big u64 = 1 << 63\nconst c = u64(1) << 63\nfn main() {\n    var a any = big\n    var a2 any = c\n    s := []any{big, c, u64(18446744073709551615)}\n    msg := \"v={c}\"\n    fmt.println(a, a2, s, big, c, msg, u64(18446744073709551615))\n}\n",
        "import \"fmt\"\nfn main() {\n    const local_big u64 = 1 << 63\n    const local_c = u64(1) << 63\n    fmt.println(local_big, local_c, \"v={local_c}\")\n}\n",
        "import \"fmt\"\nconst big = 1 << 62\nfn shadow(big int) {\n    fmt.println(big * 4)\n}\nfn main() {\n    shadow(3)\n    xs := []int{1, 2}\n    for _, big in xs { fmt.println(big * 4) }\n    for big in 0..3 { s := \"v={big * 4}\"\n fmt.println(big * 4, s) }\n    f := fn(big int) { fmt.println(big * 4) }\n    f(2)\n    var o option[int] = some(2)\n    match o { some(big): { fmt.println(big * 4) }\n none: {} }\n}\n",
        "import \"fmt\"\nconst fbig = 1e308\nfn show(fbig f64) {\n    fmt.println(fbig * 10.0, any(fbig * 10.0))\n}\nfn main() {\n    show(1.5)\n    { fbig := 2.0\n fmt.println(fbig * 10.0, any(fbig * 10.0)) }\n    fs := []f64{1.0, 2.0}\n    for _, fbig in fs { fmt.println(fbig * 10.0) }\n    g := fn(fbig f64) { fmt.println(fbig * 10.0) }\n    g(3.0)\n}\n",
        "import \"fmt\"\nfn main() {\n    var o option[u8] = some(200)\n    var o2 option[u64] = some(u64(1 << 63))\n    fmt.println(o.is_some(), o2.is_some())\n}\n",
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "expected accept for:\n{src}\ngot: {:?}", r.diagnostics);
    }
}
