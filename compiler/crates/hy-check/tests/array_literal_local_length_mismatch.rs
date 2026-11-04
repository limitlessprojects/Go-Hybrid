//! array_literal_local_length_mismatch.rs — Row 13 BUILDABLE.
//!
//! #2771 rejects `[3]int` vs `[4]int` for locals declared with an explicit
//! type, but a local initialised from an array literal (`a := [3]int{..}`,
//! `a := [...]int{..}`) recorded no type spelling, so comparing/assigning it
//! against an array of another length or element type passed `hyb check` and
//! failed `go build` (`mismatched types [3]int and [4]int`).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn prog(body: &str) -> String {
    format!("fn main() {{\n{body}\n}}\n")
}

#[test]
fn literal_initialised_arrays_of_different_type_reject() {
    for (name, body) in [
        ("lit == var", "    a := [3]int{1, 2, 3}\n    var b [4]int\n    _ = a == b"),
        ("var == lit", "    var a [3]int\n    b := [4]int{1, 2, 3, 4}\n    _ = a == b"),
        ("lit != lit", "    a := [3]int{1, 2, 3}\n    b := [4]int{1, 2, 3, 4}\n    _ = a != b"),
        ("lit = var assign", "    a := [3]int{1, 2, 3}\n    var b [4]int\n    b = a\n    _ = b"),
        ("lit = lit assign", "    a := [3]int{1, 2, 3}\n    b := [4]int{1, 2, 3, 4}\n    b = a\n    _ = b"),
        ("[...] vs [4]", "    a := [...]int{1, 2, 3}\n    b := [4]int{1, 2, 3, 4}\n    _ = a == b"),
        ("elem type differs", "    a := [3]int{1, 2, 3}\n    b := [3]int64{1, 2, 3}\n    _ = a == b"),
    ] {
        let r = check(&prog(body));
        assert_eq!(r.verdict, Verdict::Reject, "{name}: {:?}", r.diagnostics);
    }
}

#[test]
fn matching_literal_arrays_stay_accepted() {
    for (name, body) in [
        ("same len", "    a := [3]int{1, 2, 3}\n    b := [3]int{4, 5, 6}\n    _ = a == b"),
        ("[...] same len", "    a := [...]int{1, 2, 3}\n    b := [3]int{1, 2, 3}\n    _ = a == b"),
        ("assign same len", "    a := [3]int{1, 2, 3}\n    var b [3]int\n    b = a\n    _ = b"),
        ("slice literal", "    a := []int{1, 2}\n    b := []int{1, 2, 3}\n    a = b\n    _ = a"),
    ] {
        let r = check(&prog(body));
        assert_eq!(r.verdict, Verdict::Accept, "{name}: {:?}", r.diagnostics);
    }
}
