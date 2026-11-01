//! Regression tests for type-elided composite literal slot & field checking
//! (`elided_literal.rs`, `HYB-E1002`).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|d| panic!("must parse: {d:?}"));
    check_file(&ast)
}

fn e1002_messages(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error && d.code == "HYB-E1002")
        .map(|d| d.message.clone())
        .collect()
}

const POINT: &str = "struct point { x int }\n";

#[test]
fn untyped_slice_lit_in_non_slice_or_aliased_slot_rejects_elided_struct() {
    for (label, body) in [
        (
            "untyped slice in []point slot",
            "fn main() { xs := []point{[{x: 1}]}\n fmt.println(\"{len(xs)}\") }",
        ),
        (
            "untyped slice in map[string]point value slot",
            "fn main() { m := map[string]point{\"a\": [{x: 1}]}\n fmt.println(\"{len(m)}\") }",
        ),
        (
            "extra nesting depth in [][]point",
            "fn main() { g := [][]point{[[{x: 1}]]}\n fmt.println(\"{len(g)}\") }",
        ),
        (
            "type-aliased inner slice type []row{[{x: 1}]}",
            "type row = []point\nfn main() { g := []row{[{x: 1}]}\n fmt.println(\"{len(g)}\") }",
        ),
    ] {
        let src = format!("{POINT}{body}\n");
        let r = check(&src);
        let msgs = e1002_messages(&r);
        assert!(
            msgs.iter().any(|m| m.contains("type-elided struct literal")),
            "{label} must reject with HYB-E1002 for:\n{src}\ngot: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn elided_struct_lit_in_non_struct_element_slot_rejects_e1002() {
    for (label, body) in [
        (
            "elided struct in []int",
            "fn main() { xs := []int{{x: 1}}\n fmt.println(\"{len(xs)}\") }",
        ),
        (
            "empty elided braces in []int",
            "fn main() { xs := []int{{}}\n fmt.println(\"{len(xs)}\") }",
        ),
        (
            "elided struct in map[string]int value",
            "fn main() { m := map[string]int{\"a\": {x: 1}}\n fmt.println(\"{len(m)}\") }",
        ),
        (
            "elided struct in []any",
            "fn main() { xs := []any{{x: 1}}\n fmt.println(\"{len(xs)}\") }",
        ),
        (
            "elided struct in []option[point]",
            "fn main() { xs := []option[point]{{x: 1}}\n fmt.println(\"{len(xs)}\") }",
        ),
        (
            "elided struct in []enum",
            "enum color { red, blue }\nfn main() { xs := []color{{}}\n fmt.println(\"{len(xs)}\") }",
        ),
        (
            "elided struct in []*int",
            "fn main() { xs := []*int{{}}\n fmt.println(\"{len(xs)}\") }",
        ),
        (
            "elided struct in []**point",
            "fn main() { xs := []**point{{x: 1}}\n fmt.println(\"{len(xs)}\") }",
        ),
        (
            "elided struct in type-aliased pointer slice []ptr_pt",
            "type ptr_pt = *point\nfn main() { xs := []ptr_pt{{x: 1}}\n fmt.println(\"{len(xs)}\") }",
        ),
        (
            "non-empty elided struct fields in [][]int",
            "fn main() { xs := [][]int{{x: 1}}\n fmt.println(\"{len(xs)}\") }",
        ),
        (
            "elided struct inside array-length type expression",
            "struct sink { n int }\nfn main() { var a [fn() int { v := {n: 1}\n return v.n }()]int\n fmt.println(\"{len(a)}\") }",
        ),
    ] {
        let src = format!("{POINT}{body}\n");
        let r = check(&src);
        let msgs = e1002_messages(&r);
        assert!(
            msgs.iter().any(|m| m.contains("type-elided struct literal")),
            "{label} must reject with HYB-E1002 for:\n{src}\ngot: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn elided_struct_lit_unknown_and_duplicate_fields_reject_e1002() {
    for (label, needle, body) in [
        (
            "unknown field in []point elided literal",
            "unknown field `y` in `point` literal",
            "fn main() { xs := []point{{y: 1}}\n fmt.println(\"{len(xs)}\") }",
        ),
        (
            "duplicate field in []point elided literal",
            "duplicate field `x` in `point` literal",
            "fn main() { xs := []point{{x: 1, x: 2}}\n fmt.println(\"{len(xs)}\") }",
        ),
        (
            "unknown field in []*point elided literal",
            "unknown field `z` in `point` literal",
            "fn main() { xs := []*point{{z: 1}}\n fmt.println(\"{len(xs)}\") }",
        ),
        (
            "unknown field in map[string]point elided literal",
            "unknown field `bad` in `point` literal",
            "fn main() { m := map[string]point{\"a\": {bad: 1}}\n fmt.println(\"{len(m)}\") }",
        ),
    ] {
        let src = format!("{POINT}{body}\n");
        let r = check(&src);
        let msgs = e1002_messages(&r);
        assert!(
            msgs.iter().any(|m| m.contains(needle)),
            "{label} must reject with `{needle}` for:\n{src}\ngot: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn valid_elided_composite_literals_stay_accepted() {
    for body in [
        "fn main() { xs := []*point{{x: 1}, {x: 2}}\n fmt.println(\"{len(xs)}\") }",
        "type pt = point\nfn main() { xs := []pt{{x: 1}}\n fmt.println(\"{len(xs)}\") }",
        "fn main() { m := map[string][]point{\"a\": [{x: 1}]}\n fmt.println(\"{len(m)}\") }",
        "fn main() { g := [][]int{{}}\n m := []map[string]int{{}}\n fmt.println(\"{len(g)} {len(m)}\") }",
    ] {
        let src = format!("{POINT}{body}\n");
        let r = check(&src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "must accept:\n{src}\ngot: {:?}",
            r.diagnostics
        );
    }
}
