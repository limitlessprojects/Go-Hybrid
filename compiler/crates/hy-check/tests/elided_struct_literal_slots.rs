//! A type-elided struct literal `{...}` outside a composite element slot
//! must be HYB-E1002 at check time (GUARANTEES Row 13). Before the
//! elided_literal pass each rejected program below was `OK (0 diagnostics)`
//! and then `hyb build` refused it with `emitter (MVP-0): not yet
//! supported: type-elided struct literal — spell the type`.

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

fn assert_rejects(src: &str, needle: &str) {
    let r = check(src);
    let errs = errors(&r);
    assert_eq!(r.verdict, Verdict::Reject, "must reject:\n{src}");
    assert!(
        errs.iter().any(|e| e.starts_with("HYB-E1002") && e.contains(needle)),
        "expected HYB-E1002 mentioning `{needle}` for:\n{src}\ngot {errs:?}"
    );
}

fn assert_accepts(src: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "must accept:\n{src}\ngot {:?}",
        errors(&r)
    );
}

const POINT: &str = "struct point { x int }\n";

#[test]
fn elided_struct_literal_outside_an_element_slot_rejects() {
    for body in [
        "fn main() { v := {x: 1}\n fmt.println(\"{v.x}\") }",
        "fn show(p point) int { return p.x }\nfn main() { fmt.println(\"{show({x: 1})}\") }",
        "fn mk() point { return {x: 1} }\nfn main() { fmt.println(\"{mk().x}\") }",
        "fn main() { var v point = {x: 1}\n fmt.println(\"{v.x}\") }",
        "fn main() { var v point\n v = {x: 2}\n fmt.println(\"{v.x}\") }",
        "fn main() { ch := make(chan point, 1)\n ch <- {x: 4}\n fmt.println(\"{(<-ch).x}\") }",
        "fn main() { xs := []point{}\n xs = append(xs, {x: 1})\n fmt.println(\"{len(xs)}\") }",
        "struct wrap { inner point }\nfn main() { w := wrap{inner: {x: 3}}\n fmt.println(\"{w.inner.x}\") }",
        // an untyped slice literal gives its items no expected type
        "fn main() { xs := [point{x: 1}, {x: 2}]\n fmt.println(\"{len(xs)}\") }",
        // closure bodies are emitted too
        "fn main() { f := fn() point { return {x: 1} }\n fmt.println(\"{f().x}\") }",
    ] {
        assert_rejects(&format!("{POINT}{body}\n"), "type-elided struct literal");
    }
}

#[test]
fn elided_literals_in_element_slots_stay_accepted() {
    for body in [
        // typed struct head
        "fn main() { ps := [point{x: 1}, point{x: 2}]\n fmt.println(\"{len(ps)}\") }",
        // elided struct items of typed slice/array/map literals
        "fn main() { ps := []point{{x: 1}, ({x: 2})}\n m := map[string]point{\"a\": {x: 1}}\n fmt.println(\"{len(ps)} {len(m)}\") }",
        "fn main() { arr := [2]point{{x: 1}, {x: 2}}\n fmt.println(\"{arr[1].x}\") }",
        // nested elided literals inherit through typed parents
        "fn main() { g := [][]point{[{x: 1}], [{x: 2}, {x: 3}]}\n a := 1\n h := [][]int{[a, 2]}\n m := map[string][]int{\"k\": [a]}\n fmt.println(\"{len(g)} {len(h)} {len(m)}\") }",
        // typed struct literals anywhere; elided ones as map keys
        "fn show(p point) int { return p.x }\nfn main() { v := point{x: 1}\n m := map[point]int{{x: 1}: 1}\n fmt.println(\"{show(point{x: 2})} {v.x} {len(m)}\") }",
    ] {
        assert_accepts(&format!("{POINT}{body}\n"));
    }
    assert_accepts(
        "struct point { x int }\nconst origin = point{x: 0}\nconst pts = []point{{x: 1}}\nfn main() { fmt.println(\"{origin.x} {len(pts)}\") }\n",
    );
}
