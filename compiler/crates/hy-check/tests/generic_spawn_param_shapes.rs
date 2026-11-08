use hy_check::check_file;
use hy_syntax::parse_file;

#[test]
fn generic_spawn_enforces_nil_and_auto_address() {
    let nil_src = "fn worker[T](x T) {}\nfn main() {\n    managed(\"audit\", \"actor\") {\n        task worker[int](nil)\n    }\n}\n";
    let r1 = check_file(&parse_file("nil.hy", nil_src).unwrap());
    assert!(r1.diagnostics.iter().any(|d| d.code == "HYB-E4001"));
    let ptr_src = "fn bump[T](tag T, p *int) {}\nfn main() {\n    n := 1\n    task bump[int](0, n)\n}\n";
    let r2 = check_file(&parse_file("ptr.hy", ptr_src).unwrap());
    assert!(r2.diagnostics.iter().any(|d| d.code == "HYB-E3001"));
}
