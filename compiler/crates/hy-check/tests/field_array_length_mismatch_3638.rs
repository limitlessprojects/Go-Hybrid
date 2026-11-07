use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn comparing_array_typed_struct_fields_of_different_lengths_rejects_3638() {
    let repro = r#"
struct s { a [2]int }
struct t { a [3]int }

fn main() {
    x := s{}
    y := t{}
    _ = x.a == y.a
}
"#;
    let r = check_file(&parse_file("repro.hy", repro).unwrap());
    assert_eq!(r.verdict, Verdict::Reject, "x.a == y.a must reject with HYB-E1002");
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
        "expected HYB-E1002 on array length mismatch: {:?}",
        r.diagnostics
    );

    // Negative control: same array length must accept cleanly
    let accept_src = r#"
struct s1 { a [2]int }
struct s2 { a [2]int }

fn main() {
    x := s1{}
    y := s2{}
    _ = x.a == y.a
}
"#;
    let r = check_file(&parse_file("accept.hy", accept_src).unwrap());
    assert_eq!(r.verdict, Verdict::Accept, "matching array lengths must accept: {:?}", r.diagnostics);
}
