//! An untyped slice literal `[h, a, b]` lowers to `[]H{h, a, b}` with `H`
//! inferred from its first item (hy-emit `infer_slice_head`). Every later
//! item must be assignable to `H`, as in the spelled-out `[]H{...}`.
//! Before this check `[1, "a"]`, `[1, 2.5]`, `[1, x]` (`x float64`),
//! `["a", nil]`, `["a", 'b']`, `[true, 1]`, and `[p{..}, q{..}]` were
//! `OK (0 diagnostics)` and then failed `go build` with `cannot use … in
//! array or slice literal`; `[nil]` passed check and was refused by
//! `hyb build` (`nil in an untyped slice literal`).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|d| panic!("must parse: {d:?}"));
    check_file(&ast)
}

fn errors(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect()
}

fn program(body: &str) -> String {
    format!("struct p {{ x int }}\nstruct q {{ x int }}\nfn main() {{ {body}\n fmt.println(\"{{len(xs)}}\") }}\n")
}

#[test]
fn later_items_must_match_the_inferred_head() {
    for (body, needle) in [
        ("xs := [1, \"a\"]", "`\"a\"` (type `string`) as `int`"),
        ("xs := [1, 2.5]", "`2.5` (type `float64`) as `int`"),
        ("x := 2.5\n xs := [1, x]", "`x` (type `float64`) as `int`"),
        ("xs := [\"a\", 'b']", "as `string`"),
        ("xs := [true, 1]", "`1` (type `int`) as `bool`"),
        ("xs := [-1, \"a\"]", "as `int`"),
        ("xs := [(1.5), true]", "as `float64`"),
        ("xs := [p{x: 1}, q{x: 2}]", "as `p`"),
        ("xs := [\"a\", nil]", "`nil` as `string`"),
        ("xs := [nil]", "cannot start with `nil`"),
        ("xs := [\"a\"]\n for _, s in [\"b\", 1] { fmt.println(s) }", "`1` (type `int`) as `string`"),
    ] {
        let src = program(body);
        let r = check(&src);
        let errs = errors(&r);
        assert_eq!(r.verdict, Verdict::Reject, "must reject:\n{src}");
        assert!(
            errs.iter().any(|e| e.starts_with("HYB-E1002") && e.contains(needle)),
            "expected HYB-E1002 containing {needle:?} for:\n{src}\ngot {errs:?}"
        );
    }
}

#[test]
fn assignable_items_and_slotted_literals_stay_accepted() {
    for body in [
        "xs := [1, 2, -3, (4)]",
        "xs := [1.5, 2]",          // untyped int constant in a float64 slot
        "xs := [1, 2.0]",          // integral float constant in an int slot (Go accepts)
        "xs := ['a', 1]",          // untyped int constant in a rune slot
        "x := 3\n xs := [1, x, x + 1]",
        "xs := [true, 1 == 1]",
        "xs := [p{x: 1}, p{x: 2}]",
        // in an element slot the literal takes the EXPECTED type, not its head
        "xs := [][]f64{[1, 2.5]}\n ys := map[string][]f64{\"k\": [1, 2.5]}\n fmt.println(\"{len(ys)}\")",
        "xs := [2][]f64{[1, 2.5], [3]}",
    ] {
        let src = program(body);
        let r = check(&src);
        assert_eq!(r.verdict, Verdict::Accept, "must accept:\n{src}\ngot {:?}", errors(&r));
    }
}
