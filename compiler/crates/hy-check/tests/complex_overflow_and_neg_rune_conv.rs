use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    check_file(&parse_file("t.hy", src).expect("snippet parses"))
}

#[test]
fn complex_shift_const_overflow_and_neg_rune_conversions() {
    for src in [
        "const z complex64 = 1e40\nfn main() { _ = z }\n",
        "fn main() {\n    var z c64 = 1e40\n    _ = z\n}\n",
        "fn main() {\n    var z complex128 = 1e400\n    _ = z\n}\n",
        "fn main() {\n    var z complex64\n    z = 1e40\n    _ = z\n}\n",
        "fn main() {\n    var z complex64 = complex(0, 1e40)\n    _ = z\n}\n",
        "fn main() {\n    var x float32 = 1 << 200\n    _ = x\n}\n",
        "fn f() complex64 { return 1 << 200 }\nfn main() { _ = f() }\n",
    ] {
        let r = check(src);
        assert!(r.verdict == Verdict::Reject && r.diagnostics.iter().any(|d| d.code == "HYB-E6002"), "{src}");
    }
    for src in [
        "fn main() {\n    x := uint(-'a')\n    _ = x\n}\n",
        "fn main() {\n    x := uintptr(-'a')\n    _ = x\n}\n",
        "type my_u8 = u8\nfn main() {\n    x := my_u8(-'a')\n    _ = x\n}\n",
    ] {
        let r = check(src);
        assert!(r.verdict == Verdict::Reject && r.diagnostics.iter().any(|d| d.code == "HYB-E6006"), "{src}");
    }
    for src in [
        "fn main() {\n    var z complex64 = 3.4e38\n    var w complex128 = 1e300\n    var f float32 = 1 << 127\n    a := u8(-'a')\n    b := i16(-'a')\n    _, _, _, _, _ = z, w, f, a, b\n}\n",
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Accept, "unexpected reject: {:?}", r.diagnostics);
    }
}
