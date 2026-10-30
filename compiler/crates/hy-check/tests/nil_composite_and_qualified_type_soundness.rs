//! nil_composite_and_qualified_type_soundness.rs — tests for HYB-E1002 rejection of:
//! 1. `nil` in non-nilable composite literals (array, slice, map values) (#2036).
//! 2. Built-in types (`any`, `error`) and qualified package types (`http.Client`, `sync.Mutex`)
//!    appearing in value positions (#2041).
//!
//! Row 13 BUILDABLE invariant (GUARANTEES.md).

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

fn assert_e1002(src: &str) {
    let cs = codes(src);
    assert!(
        cs.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 in {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(
        cs.is_empty(),
        "expected 0 diagnostics, got {cs:?} for:\n{src}"
    );
}

// ---------------------------------------------------------------------------
// Issue #2036: nil in non-nilable composite literals
// ---------------------------------------------------------------------------

#[test]
fn rejects_nil_in_non_nilable_slice_elements_2036() {
    assert_e1002("fn main() {\n    s := []int{nil}\n    _ = s\n}\n");
    assert_e1002("fn main() {\n    s := []bool{nil}\n    _ = s\n}\n");
    assert_e1002("fn main() {\n    s := []string{nil}\n    _ = s\n}\n");
    assert_e1002("fn main() {\n    s := []float64{nil}\n    _ = s\n}\n");
}

#[test]
fn rejects_nil_in_non_nilable_array_elements_2036() {
    assert_e1002("fn main() {\n    a := [2]int{nil, 1}\n    _ = a\n}\n");
    assert_e1002("fn main() {\n    a := [1]string{nil}\n    _ = a\n}\n");
    assert_e1002("fn main() {\n    a := [1]bool{nil}\n    _ = a\n}\n");
}

#[test]
fn rejects_nil_in_non_nilable_map_values_2036() {
    assert_e1002("fn main() {\n    m := map[string]int{\"k\": nil}\n    _ = m\n}\n");
    assert_e1002("fn main() {\n    m := map[string]string{\"k\": nil}\n    _ = m\n}\n");
    assert_e1002("fn main() {\n    m := map[string]bool{\"k\": nil}\n    _ = m\n}\n");
}

#[test]
fn accepts_nil_in_nilable_composite_elements_2036() {
    assert_ok("fn main() {\n    s := []*int{nil}\n    _ = s\n}\n");
    assert_ok("fn main() {\n    s := [][]int{nil}\n    _ = s\n}\n");
    assert_ok("fn main() {\n    s := []any{nil}\n    _ = s\n}\n");
    assert_ok("fn main() {\n    m := map[string]*int{\"k\": nil}\n    _ = m\n}\n");
    assert_ok("fn main() {\n    m := map[string][]int{\"k\": nil}\n    _ = m\n}\n");
}

// ---------------------------------------------------------------------------
// Issue #2041: type names in value positions
// ---------------------------------------------------------------------------

#[test]
fn rejects_builtin_any_and_error_in_value_positions_2041() {
    assert_e1002("fn main() {\n    x := any\n    _ = x\n}\n");
    assert_e1002("fn main() {\n    y := error\n    _ = y\n}\n");
    assert_e1002("fn inspect(val any) {}\nfn main() {\n    inspect(any)\n}\n");
    assert_e1002("fn f() any {\n    return any\n}\n");
}

#[test]
fn rejects_qualified_type_names_in_value_positions_2041() {
    assert_e1002("import \"net/http\"\nfn main() {\n    c := http.Client\n    _ = c\n}\n");
    assert_e1002("import \"sync\"\nfn main() {\n    m := sync.Mutex\n    _ = m\n}\n");
    assert_e1002("import \"time\"\nfn main() {\n    d := time.Duration\n    _ = d\n}\n");
}

#[test]
fn accepts_valid_type_positions_and_package_calls_2041() {
    assert_ok("fn main() {\n    var x any = 1\n    var e error = nil\n    _ = x\n    _ = e\n}\n");
    assert_ok("import \"net/http\"\nfn main() {\n    var c http.Client\n    _ = c\n}\n");
    assert_ok("import \"sync\"\nfn main() {\n    var m sync.Mutex\n    _ = m\n}\n");
    assert_ok("import \"net/http\"\nfn main() {\n    p := new(http.Client)\n    _ = p\n}\n");
}
