//! Regression tests for issues #3696 (map literal duplicate-key constant folding
//! across wrapped int conversions, isize/usize/float conversions, -0.0, len("..."),
//! and type aliases) and #3678 (constant operands overflowing inferred-int local
//! variables `x := 1`, `var x = 1`, `let x = 1`).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

#[test]
fn map_lit_duplicate_wrapped_int_word_and_float_conversion_keys_rejected_3696() {
    let cases = [
        "import \"fmt\"\nfn main() {\n    m := map[u8]int{u8(300): 1, 44: 2}\n    fmt.println(m)\n}\n",
        "import \"fmt\"\nfn main() {\n    m := map[i8]int{i8(200): 1, -56: 2}\n    fmt.println(m)\n}\n",
        "import \"fmt\"\nfn main() {\n    m := map[u8]int{u8(-1): 1, 255: 2}\n    fmt.println(m)\n}\n",
        "import \"fmt\"\nfn main() {\n    m := map[usize]int{usize(1): 1, 1: 2}\n    fmt.println(m)\n}\n",
        "import \"fmt\"\nfn main() {\n    m := map[isize]int{isize(1): 1, 1: 2}\n    fmt.println(m)\n}\n",
        "import \"fmt\"\nfn main() {\n    m := map[f64]int{f64(1.5): 1, 1.5: 2}\n    fmt.println(m)\n}\n",
        "import \"fmt\"\nfn main() {\n    m := map[f64]int{f64(1): 1, 1.0: 2}\n    fmt.println(m)\n}\n",
        "import \"fmt\"\nfn main() {\n    m := map[f32]int{f32(1.5): 1, float32(1.5): 2}\n    fmt.println(m)\n}\n",
        "import \"fmt\"\nfn main() {\n    m := map[f64]int{0.0: 1, -0.0: 2}\n    fmt.println(m)\n}\n",
        "import \"fmt\"\nfn main() {\n    m := map[int]int{len(\"ab\"): 1, 2: 2}\n    fmt.println(m)\n}\n",
        "import \"fmt\"\ntype key_t = int\nfn main() {\n    m := map[key_t]int{key_t(1): 1, 1: 2}\n    fmt.println(m)\n}\n",
    ];
    for src in cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected reject for:\n{src}");
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.code == "HYB-E1002" && d.message.contains("duplicate key")),
            "expected HYB-E1002 duplicate key diagnostic, got {:?} for:\n{src}",
            r.diagnostics
        );
    }
}

#[test]
fn inferred_int_local_rejects_overflowing_constant_operands_3678() {
    let reject_cases = [
        "import \"fmt\"\nfn main() {\n    x := 1\n    x += 9223372036854775808\n    fmt.println(x)\n}\n",
        "import \"fmt\"\nfn main() {\n    var x = 1\n    x ^= 1 << 70\n    fmt.println(x)\n}\n",
        "import \"fmt\"\nfn main() {\n    x := 1\n    x = 18446744073709551616 % x\n    fmt.println(x)\n}\n",
        "import \"fmt\"\nfn main() {\n    let x = 1\n    fmt.println(x == 9223372036854775808)\n}\n",
        "import \"fmt\"\nfn main() {\n    x := 1\n    fmt.println(x + 9223372036854775808)\n}\n",
        "import \"fmt\"\nfn main() {\n    f := fn() {\n        x := 1\n        x += 9223372036854775808\n        fmt.println(x)\n    }\n    f()\n}\n",
    ];
    for src in reject_cases {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected reject for:\n{src}");
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E6002"),
            "expected HYB-E6002 diagnostic, got {:?} for:\n{src}",
            r.diagnostics
        );
    }

    let accept_cases = [
        "import \"fmt\"\nfn main() {\n    x := 1\n    x += 9223372036854775807\n    fmt.println(x == 42)\n}\n",
        "import \"fmt\"\nfn main() {\n    const c = 1\n    var u u64 = c + 9223372036854775808\n    fmt.println(u)\n}\n",
        "import \"fmt\"\nfn main() {\n    x := 1 << ('b' - 'a')\n    x += 1 << 40\n    fmt.println(x)\n}\n",
    ];
    for src in accept_cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "expected accept for:\n{src}\ndiagnostics: {:?}",
            r.diagnostics
        );
    }
}
