#[test]
fn array_len_validates_builtin_args_e2005() { for s in ["fn main() { var a [len(1)]int\n_ = a }\n", "fn main() { b := [cap(\"hi\") + 1]int{0}\n_ = b }\n", "type a_arr = [len()]int\nfn main() {}\n"] { let r = hy_check::check_file(&hy_syntax::parse_file("t.hy", s).unwrap()); assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E2005"), "{:?}", r.diagnostics); } }
