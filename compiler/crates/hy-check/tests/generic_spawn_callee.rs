use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn generic_top_level_spawn_and_shadowed_closure() {
    let ok = parse_file("ok.hy", "fn worker[T](x T) {}\nfn main() { task worker[int](42) }\n").unwrap();
    assert_eq!(check_file(&ok).verdict, Verdict::Accept);
    let rej = parse_file("rej.hy", "fn main() {\n    c := fn(x int) {}\n    task c[int](1)\n}\n").unwrap();
    let r = check_file(&rej);
    assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E3001"));
}
