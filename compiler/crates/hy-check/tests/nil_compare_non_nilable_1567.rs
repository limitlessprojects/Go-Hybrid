//! #1567: `x == nil` on a type with no nil passed `hyb check`, then failed
//! `go build` (`mismatched types int and untyped nil`).

use hy_check::check_file;
use hy_syntax::parse_file;

fn e1002(body: &str) -> usize {
    let src = format!("struct s {{\n    a int\n}}\n\nfn main() {{\n{}\n}}\n", body);
    let ast = parse_file("inline.hy", &src).expect("must parse");
    let r = check_file(&ast);
    r.diagnostics.iter().filter(|d| d.code == "HYB-E1002").count()
}

#[test]
fn nil_compare_on_non_nilable_types_is_rejected() {
    assert_eq!(e1002("    x := 1\n    _ = x == nil"), 1);
    assert_eq!(e1002("    x := \"a\"\n    _ = nil != x"), 1);
    assert_eq!(e1002("    x := true\n    _ = x != nil"), 1);
    assert_eq!(e1002("    x := 1.5\n    _ = nil == x"), 1);
    assert_eq!(e1002("    v := s{a: 1}\n    _ = v == nil"), 1);
    assert_eq!(e1002("    a := [2]int{1, 2}\n    _ = a == nil"), 1);
}

#[test]
fn nil_compare_on_nilable_types_stays_accepted() {
    let body = concat!(
        "    var xs []int\n    var p *s\n    var a any = 1\n    var e error\n",
        "    f := fn() {}\n    ch := make(chan int)\n",
        "    _ = xs == nil || p == nil || a == nil || e == nil || f == nil || nil != ch",
    );
    assert_eq!(e1002(body), 0);
}
