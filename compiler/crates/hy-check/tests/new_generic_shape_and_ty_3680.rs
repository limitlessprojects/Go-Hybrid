fn check(src: &str) -> Vec<hy_diag::Diagnostic> { hy_check::check_file(&hy_syntax::parse_file("t.hy", src).unwrap()).diagnostics }
#[test]
fn new_generic_shape_and_ty_3680() {
    assert!(check("struct holder {\n    val u8\n    opt option[int]\n}\nfn main() {\n    ch := make(chan int, 2)\n    a := new([2]int{})\n    for x in a {\n        ch <- x\n    }\n    p := new(holder)\n    ch <- p.opt.unwrap()\n}\n").is_empty());
    assert!(check("struct holder {\n    val u8\n}\nfn main() {\n    p := new(holder)\n    p.val = 300\n}\n").iter().any(|d| d.code == "HYB-E6002"));
}
