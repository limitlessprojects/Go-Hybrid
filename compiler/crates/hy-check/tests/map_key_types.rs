//! BUILDABLE (GUARANTEES Row 13) — invalid map key types. `hyb check`
//! returned 0 diagnostics for `map[[]int]V`, `map[map[K]V]V2`, `map[fn()]V`,
//! and arrays/structs carrying one, in any type position (literal, `var`,
//! param, struct field), and the emitted Go failed `go build` with
//! `invalid map key type`.
//!
//! Every reject case was first reproduced as a real `go build` failure on
//! the emitted Go (go1.26.9), and every accept case builds.

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("parses");
    check_file(&ast)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn assert_code(src: &str, want: &str) {
    let cs = codes(src);
    assert_eq!(cs, vec![want.to_string()], "for:\n{src}");
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(
        cs.is_empty(),
        "expected 0 diagnostics, got {cs:?} for:\n{src}"
    );
}

// ---------------------------------------------------------------------------
// invalid map key type
// ---------------------------------------------------------------------------

#[test]
fn slice_map_and_func_keys_are_rejected() {
    assert_code(
        "fn main() {\n    m := map[[]int]string{}\n    fmt.println(\"{len(m)}\")\n}\n",
        "HYB-E2006",
    );
    assert_code(
        "fn main() {\n    var m map[fn()]int\n    fmt.println(\"{len(m)}\")\n}\n",
        "HYB-E2006",
    );
    assert_code(
        "fn f(m map[map[string]int]bool) int {\n    return len(m)\n}\n\nfn main() {\n    fmt.println(\"{f(nil)}\")\n}\n",
        "HYB-E2006",
    );
}

#[test]
fn keys_carrying_an_incomparable_part_are_rejected() {
    // An array of slices, and a struct with a slice field.
    assert_code(
        "fn main() {\n    m := map[[2][]int]int{}\n    fmt.println(\"{len(m)}\")\n}\n",
        "HYB-E2006",
    );
    assert_code(
        "struct key {\n    parts []string\n}\n\nfn main() {\n    m := map[key]int{}\n    fmt.println(\"{len(m)}\")\n}\n",
        "HYB-E2006",
    );
    // A struct field declared with a bad map type.
    assert_code(
        "struct index {\n    by []int\n    seen map[[]int]bool\n}\n\nfn main() {\n    i := index{}\n    fmt.println(\"{len(i.by)}\")\n}\n",
        "HYB-E2006",
    );
}

#[test]
fn comparable_keys_stay_accepted() {
    assert_ok("fn main() {\n    m := map[string]int{\"a\": 1}\n    fmt.println(\"{len(m)}\")\n}\n");
    assert_ok("fn main() {\n    m := map[[2]int]int{}\n    fmt.println(\"{len(m)}\")\n}\n");
    assert_ok("struct pt {\n    x int\n    y int\n}\n\nfn main() {\n    m := map[pt]string{}\n    fmt.println(\"{len(m)}\")\n}\n");
    assert_ok("fn main() {\n    m := map[*int]int{}\n    fmt.println(\"{len(m)}\")\n}\n");
    // Slices/maps as VALUES are fine.
    assert_ok("fn main() {\n    m := map[string][]int{}\n    fmt.println(\"{len(m)}\")\n}\n");
}
