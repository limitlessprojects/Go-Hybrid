use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    check_file(&parse_file("t.hy", src).unwrap())
        .diagnostics
        .iter()
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn array_conversion_with_mismatched_elem_rejects_e1002() {
    let src = "fn main() {\n    var s []string = []string{\"a\", \"b\", \"c\"}\n    var a = [3]int(s)\n    println(a[0])\n}\n";
    assert!(codes(src).iter().any(|c| c == "HYB-E1002"), "{:?}", codes(src));
    let r = check_file(&parse_file("t.hy", src).unwrap());
    let msgs: Vec<_> = r.diagnostics.iter().map(|d| d.message.clone()).collect();
    assert!(msgs.iter().any(|m| m.contains("incompatible element types")), "{msgs:?}");
    let src = "fn main() {\n    var f [2]float64\n    var a = [2]int(f)\n    println(a[0])\n}\n";
    assert!(codes(src).iter().any(|c| c == "HYB-E1002"), "{:?}", codes(src));
}

#[test]
fn array_conversion_with_identical_elem_is_accepted() {
    let src = "fn main() {\n    var s []int = []int{1, 2, 3}\n    var a = [3]int(s)\n    println(a[0])\n}\n";
    let r = check_file(&parse_file("t.hy", src).unwrap());
    let msgs: Vec<_> = r.diagnostics.iter().map(|d| d.message.clone()).collect();
    assert!(!codes(src).iter().any(|c| c == "HYB-E1002"), "{msgs:?}");
}
