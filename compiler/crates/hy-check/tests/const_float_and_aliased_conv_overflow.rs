use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

#[test]
fn named_float_const_to_int_and_aliased_conversions_rejected() {
    let e6002_cases = [
        "const f = 2.5\nfn main() {\n    var x int = f\n    fmt.println(x)\n}\n",
        "const f = 1.5 + 1.0\nfn main() {\n    var x int8 = f\n    fmt.println(x)\n}\n",
        "const f = 300.0\nfn main() {\n    var x int8 = f\n    fmt.println(x)\n}\n",
        "const f = 2.5\nfn main() {\n    var x int\n    x = f\n    fmt.println(x)\n}\n",
        "fn main() {\n    var x int = 1.5 + 1.0\n    fmt.println(x)\n}\n",
        "fn main() {\n    var x int\n    x = 1.5 + 1.0\n    fmt.println(x)\n}\n",
    ];
    for src in e6002_cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected HYB-E6002 reject for:\n{src}");
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E6002"),
            "expected HYB-E6002 diagnostic, got {:?} for:\n{src}",
            r.diagnostics
        );
    }

    let e6006_cases = [
        "type my_i8 = int8\nfn main() {\n    x := my_i8(2.5)\n    fmt.println(x)\n}\n",
        "type my_i8 = int8\nfn main() {\n    x := my_i8(300)\n    fmt.println(x)\n}\n",
        "type my_f32 = float32\nfn main() {\n    x := my_f32(1e40)\n    fmt.println(x)\n}\n",
        "type bytes = []byte\nfn main() {\n    x := bytes(123)\n    fmt.println(x)\n}\n",
        "type ints = []int\nfn main() {\n    x := ints(\"abc\")\n    fmt.println(x)\n}\n",
        "type arr = [2]int\nfn main() {\n    x := arr(123)\n    fmt.println(x)\n}\n",
        "const c = 2.5\nfn main() {\n    x := int8(c)\n    fmt.println(x)\n}\n",
        "const big = 1 << 70\nfn main() {\n    x := int64(big)\n    fmt.println(x)\n}\n",
        "const big = 1 << 70\nfn main() {\n    x := u8(big)\n    fmt.println(x)\n}\n",
        "fn main() {\n    x := (int8)(2.5)\n    fmt.println(x)\n}\n",
    ];
    for src in e6006_cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected HYB-E6006 reject for:\n{src}");
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E6006"),
            "expected HYB-E6006 diagnostic, got {:?} for:\n{src}",
            r.diagnostics
        );
    }

    let accept_cases = [
        "const f = 2.0\nconst x int = f\nfn main() {\n    fmt.println(x)\n}\n",
        "type bytes = []byte\nfn main() {\n    x := bytes(\"abc\")\n    fmt.println(len(x))\n}\n",
        "type my_i8 = int8\nfn main() {\n    x := my_i8(100)\n    fmt.println(x)\n}\n",
        "fn main() {\n    x := u8(300)\n    fmt.println(x)\n}\n",
    ];
    for src in accept_cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "expected accept for:\n{src}\ngot: {:?}", r.diagnostics);
    }
}
