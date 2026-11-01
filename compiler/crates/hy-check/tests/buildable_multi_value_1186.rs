use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check_codes(src: &str) -> (Verdict, Vec<String>) {
    let res = check_file(&parse_file("test.hy", src).expect("parse"));
    let codes = res.diagnostics.into_iter().filter(|d| d.severity == Severity::Error).map(|d| d.code.to_string()).collect();
    (res.verdict, codes)
}

#[test]
fn multi_value_assign_redefine_and_forwarded_calls_enforce_e1002() {
    for src in [
        "fn two_strings() (string, string) { return \"a\", \"b\" }\nfn main() {\n var a int\n var b int\n a, b = two_strings()\n _ = a\n _ = b\n}\n",
        "fn main() {\n m := map[string]int{\"a\": 1}\n var v int\n var ok int\n v, ok = m[\"a\"]\n _ = v\n _ = ok\n}\n",
        "fn main() {\n ch := make(chan int, 1)\n var v int\n var ok string\n v, ok = <-ch\n _ = v\n _ = ok\n}\n",
        "fn main() {\n a := 1\n a, b := \"hello\", 2\n _ = a\n _ = b\n}\n",
        "fn triple() (int, int, string) { return 1, 2, \"s\" }\nfn main() {\n a, b, c := triple()\n var x int = c\n _ = a\n _ = b\n _ = x\n}\n",
        "fn pair_err() (int, string, error) { return 1, \"s\", nil }\nfn run() error {\n a, b := pair_err()?\n var x int = b\n _ = a\n _ = x\n return nil\n}\nfn main() {}\n",
        "fn two_strings() (string, string) { return \"a\", \"b\" }\nfn take_ints(x int, y int) {\n _ = x\n _ = y\n}\nfn main() {\n take_ints(two_strings())\n}\n",
        "struct sink { n int }\nfn (s sink) one(x int) {\n _ = s.n\n _ = x\n}\nfn two_ints() (int, int) { return 1, 2 }\nfn main() {\n s := sink{ n: 0 }\n s.one(two_ints())\n}\n",
    ] {
        let (verdict, codes) = check_codes(src);
        assert_eq!(verdict, Verdict::Reject, "expected reject for:\n{}", src);
        assert!(codes.iter().any(|c| c == "HYB-E1002"), "expected HYB-E1002, got {:?}", codes);
    }
    let (v_ok, c_ok) = check_codes("struct sink { n int }\nfn (s sink) pair(x int, y string) {\n _ = s.n\n _ = x\n _ = y\n}\nfn mixed() (int, string) { return 1, \"ok\" }\nfn main() {\n s := sink{ n: 0 }\n s.pair(mixed())\n var a int\n var b string\n a, b = mixed()\n a, c := 2, true\n _ = a\n _ = b\n _ = c\n}\n");
    assert_eq!(v_ok, Verdict::Accept, "{:?}", c_ok);
}
