use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("inline.hy", src).expect("must parse");
    check_file(&ast)
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code)
        .collect()
}

#[test]
fn top_level_fn_shadowing_min_preserves_slice_return_shape_at_spawn() {
    let src = r#"
fn min(s []int) []int {
    return s
}

fn worker(view []int) {
    view[0] = 1
}

fn main() {
    s := []int{0}
    task worker(min(s))
    s[0] = 2
}
"#;
    let got = codes(src);
    assert!(
        got.iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 for shadowed min(s) returning []int at task spawn, got {got:?}"
    );
}

#[test]
fn type_alias_shadowing_string_preserves_slice_conversion_shape_at_spawn() {
    let src = r#"
type string = []byte

fn worker(view string) {
    view[0] = 1
}

fn main() {
    raw := []byte{0}
    aliased := string(raw)
    task worker(aliased)
    raw[0] = 2
}
"#;
    let got = codes(src);
    assert!(
        got.iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 when type alias `string = []byte` shadows string conversion, got {got:?}"
    );
}

#[test]
fn top_level_fn_shadowing_some_is_not_classified_as_option_wrapper() {
    let src = r#"
const shared = map[string]int{"k": 1}

fn some(n int) map[string]int {
    return shared
}

fn worker(m map[string]int) {
    _ = m["k"]
}

fn main() {
    task worker(some(1))
}
"#;
    let got = codes(src);
    assert!(
        got.iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 when user fn `some` returns map[string]int at spawn, got {got:?}"
    );
}

#[test]
fn top_level_fn_shadowing_len_is_not_exempt_from_const_write_check() {
    let src = r#"
const shared = []int{0}

fn len(s []int) {
    s[0] = 1
}

fn main() {
    len(shared)
}
"#;
    let got = codes(src);
    assert!(
        got.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 when user fn `len` mutates `const` slice, got {got:?}"
    );
}

#[test]
fn unshadowed_predeclared_builtins_still_accepted() {
    let src = r#"
fn worker(n int, label string, opt option[int]) {
    _ = n
    _ = label
    _ = opt
}

fn main() {
    xs := []int{10, 20}
    task worker(min(len(xs), cap(xs)), string(65), some(7))
}
"#;
    let got = codes(src);
    assert!(
        got.is_empty(),
        "expected 0 diagnostics for unshadowed predeclared builtins, got {got:?}"
    );
}
