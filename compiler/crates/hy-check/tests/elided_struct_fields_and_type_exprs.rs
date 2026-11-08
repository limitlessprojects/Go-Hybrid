//! Soundness (GUARANTEES Row 13 — BUILDABLE):
//! Type-elided struct literals `{...}` must enforce:
//!   1. Unknown-field (U14) and duplicate-field (U15) rejection (`HYB-E1002`)
//!      when the struct head is inherited from an enclosing slice, array, map,
//!      pointer-to-struct, nested untyped slice, or type-aliased composite slot.
//!   2. Rejection (`HYB-E1002`) of type-elided struct literals inside array
//!      length expressions (`[N]T`) across declarations, statements, and
//!      expressions.
//!   3. Rejection (`HYB-E1002`) of type-elided `{...}` in curated non-struct
//!      foreign slots (`io.reader`, `http.handler`, `http.response_writer`,
//!      `time.duration`, `os.file_mode`, `net.conn`, including pointer-to-foreign
//!      `*io.reader` / `*time.duration`), while keeping buildable struct,
//!      pointer, and empty composite slots accepted.

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

fn assert_rejects_e1002(src: &str, needle: &str) {
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

#[test]
fn elided_struct_literals_reject_unknown_and_duplicate_fields() {
    for (src, needle) in [
        (
            "struct point { x int }\nfn main() {\n    ps := []point{{nope: 1}}\n    _ = ps\n}\n",
            "unknown field `nope` in `point` literal",
        ),
        (
            "struct point { x int }\nfn main() {\n    ps := []point{{x: 1, x: 2}}\n    _ = ps\n}\n",
            "duplicate field `x` in `point` literal",
        ),
        (
            "struct point { x int }\nfn main() {\n    ps := []*point{{nope: 1}}\n    _ = ps\n}\n",
            "unknown field `nope` in `point` literal",
        ),
        (
            "struct point { x int }\nfn main() {\n    arr := [1]point{{x: 1, x: 2}}\n    _ = arr\n}\n",
            "duplicate field `x` in `point` literal",
        ),
        (
            "struct point { x int }\nfn main() {\n    m := map[string]point{\"a\": {nope: 1}}\n    _ = m\n}\n",
            "unknown field `nope` in `point` literal",
        ),
        (
            "struct point { x int }\nfn main() {\n    m := map[point]int{{x: 1, x: 2}: 1}\n    _ = m\n}\n",
            "duplicate field `x` in `point` literal",
        ),
        (
            "struct point { x int }\nfn main() {\n    g := [][]point{[{nope: 1}]}\n    _ = g\n}\n",
            "unknown field `nope` in `point` literal",
        ),
        (
            "struct point { x int }\ntype pt_ptr = *point\nfn main() {\n    ps := []pt_ptr{{nope: 1}}\n    _ = ps\n}\n",
            "unknown field `nope` in `point` literal",
        ),
    ] {
        assert_rejects_e1002(src, needle);
    }
}

#[test]
fn elided_struct_literals_inside_type_array_lengths_reject() {
    for src in [
        "struct point { x int }\nfn main() {\n    var a [fn() int { v := {x: 1}\n return v.x }()]int\n    _ = a\n}\n",
        "struct point { x int }\ntype arr = [fn() int { v := {x: 1}\n return v.x }()]int\nfn main() {}\n",
        "struct point { x int }\nstruct holder { a [fn() int { v := {x: 1}\n return v.x }()]int }\nfn main() {}\n",
        "struct point { x int }\nconst c [fn() int { v := {x: 1}\n return v.x }()]int = [1]int{0}\nfn main() {}\n",
        "struct point { x int }\nfn f(a [fn() int { v := {x: 1}\n return v.x }()]int) {}\nfn main() {}\n",
        "struct point { x int }\nfn main() {\n    xs := [][fn() int { v := {x: 1}\n return v.x }()]int{}\n    _ = xs\n}\n",
    ] {
        assert_rejects_e1002(src, "type-elided struct literal");
    }
}

#[test]
fn elided_struct_literals_in_non_struct_foreign_slots_reject() {
    for src in [
        "import \"io\"\nfn main() {\n    xs := []io.reader{{}}\n    _ = xs\n}\n",
        "import \"io\"\nfn main() {\n    xs := []*io.reader{{}}\n    _ = xs\n}\n",
        "import \"net/http\"\nfn main() {\n    xs := []http.handler{{}}\n    _ = xs\n}\n",
        "import \"net/http\"\nfn main() {\n    xs := []http.response_writer{{}}\n    _ = xs\n}\n",
        "import \"time\"\nfn main() {\n    xs := []time.duration{{}}\n    _ = xs\n}\n",
        "import \"time\"\nfn main() {\n    xs := []*time.duration{{}}\n    _ = xs\n}\n",
        "import \"os\"\nfn main() {\n    xs := []os.file_mode{{}}\n    _ = xs\n}\n",
        "import \"net\"\nfn main() {\n    xs := []net.conn{{}}\n    _ = xs\n}\n",
    ] {
        assert_rejects_e1002(src, "type-elided struct literal");
    }
}

#[test]
fn valid_elided_struct_literals_and_composite_slots_stay_accepted() {
    assert_accepts(
        "struct point { x int\n y int }\n\
         type pt_ptr = *point\n\
         type row = []point\n\
         fn main() {\n\
             ps := []point{{x: 1, y: 2}, {x: 3}}\n\
             ptrs := []pt_ptr{{x: 4, y: 5}, {}}\n\
             grid := []row{[{x: 6, y: 7}], {}}\n\
             arr_ptrs := []*[2]int{{}}\n\
             m := map[point]int{{x: 1, y: 2}: 9}\n\
             fmt.println(len(ps), len(ptrs), len(grid), len(arr_ptrs), len(m))\n\
         }\n",
    );
}
