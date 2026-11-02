use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    check_file(&parse_file("test.hy", src).expect("parse")).diagnostics.into_iter().filter(|d| d.severity == Severity::Error).map(|d| d.code).collect()
}

#[test]
fn multi_value_assign_triple_and_forwarded_calls_enforce_e1002() {
    assert_eq!(codes("fn two_strings() (string, string) { return \"a\", \"b\" }\nfn take_ints(x int, y int) {\n _ = x\n _ = y\n}\nfn main() {\n var a int\n var b int\n a, b = two_strings()\n take_ints(two_strings())\n _ = a\n _ = b\n}\n").len(), 4);
    assert_eq!(codes("fn triple() (int, int, string) { return 1, 2, \"s\" }\nfn main() {\n m := map[string]int{\"a\": 1}\n var v int\n var ok int\n v, ok = m[\"a\"]\n a, b, c := triple()\n var x int = c\n _ = v\n _ = ok\n _ = a\n _ = b\n _ = x\n}\n"), vec!["HYB-E1002", "HYB-E1002"]);
    assert!(codes("fn mixed() (int, string) { return 1, \"ok\" }\nfn take_mixed(x int, y string) {\n _ = x\n _ = y\n}\nfn main() {\n take_mixed(mixed())\n var a int\n var b string\n a, b = mixed()\n _ = a\n _ = b\n}\n").is_empty());
}
