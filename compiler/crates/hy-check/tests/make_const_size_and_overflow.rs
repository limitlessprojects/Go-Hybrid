use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn make_const_identifiers_and_int_overflow_sizes() {
    for bad in [
        "const neg = -5\nfn main() { s := make([]int, neg)\n _ = s }\n",
        "const lo = 10\nconst hi = 3\nfn main() { s := make([]int, lo, hi)\n _ = s }\n",
        "fn main() { s := make([]int, 1 << 70)\n _ = s }\n",
        "fn main() { ch := make(chan int, 9223372036854775808)\n _ = ch }\n",
    ] {
        let r = check_file(&parse_file("bad.hy", bad).unwrap());
        assert_eq!(r.verdict, Verdict::Reject, "expected reject for:\n{bad}");
        assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E2005"));
    }
    let ok = "const neg = -5\nconst ok_len = 4\nconst ok_cap = 8\nfn main() {\n s := make([]int, ok_len, ok_cap)\n neg := 3\n t := make([]int, neg)\n f := fn(lo int) int { u := make([]int, lo, 4)\n return len(u) }\n _ = s\n _ = t\n _ = f(2)\n}\n";
    let r = check_file(&parse_file("ok.hy", ok).unwrap());
    assert_eq!(r.verdict, Verdict::Accept, "diags: {:?}", r.diagnostics);
}
