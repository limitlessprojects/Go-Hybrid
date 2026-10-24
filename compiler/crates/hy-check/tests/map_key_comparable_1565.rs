//! map_key_comparable_1565.rs — `map[K]V` where `K` has no `==` (issue #1565).
//!
//! `check_type_expr` walked map key/value types without ever asking whether
//! the key is comparable, so `map[[]int]int` (and `map[func()]int`,
//! `map[map[string]int]int`, a struct carrying a slice, an array of slices,
//! an alias to a slice) passed `hyb check` with 0 diagnostics and then failed
//! `go build` with `invalid map key type ...` (GUARANTEES row 13, BUILDABLE).
//!
//! The gate reuses `emit_guard::incomparable_noun` — the same predicate the
//! `==` surface uses — so only PROVEN-incomparable shapes reject: slice, map,
//! closure, an array with such an element, and a struct carrying one. `any`
//! (interface), pointers, channels, type parameters and unresolved shapes
//! stay fail-open.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_rejects(src: &str, what: &str) {
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {what}, got {cs:?}"
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {what}");
}

fn assert_accepts(src: &str, what: &str) {
    let r = check(src);
    let cs = codes(&r);
    assert!(cs.is_empty(), "expected ACCEPT for {what}, got {cs:?}");
    assert_eq!(r.verdict, Verdict::Accept, "verdict for {what}");
}

// ---------------------------------------------------------------------------
// Reject shapes — every one is `invalid map key type` in Go.
// ---------------------------------------------------------------------------

#[test]
fn slice_key_literal_rejects() {
    assert_rejects(
        "fn main() {\n    m := map[[]int]int{}\n    fmt.println(len(m))\n}\n",
        "map[[]int]int literal",
    );
}

#[test]
fn slice_key_var_decl_rejects() {
    assert_rejects(
        "fn main() {\n    var m map[[]int]int\n    fmt.println(len(m))\n}\n",
        "var m map[[]int]int",
    );
}

#[test]
fn slice_key_param_rejects() {
    assert_rejects(
        "fn f(m map[[]int]int) {\n    fmt.println(len(m))\n}\n\nfn main() {\n    f(map[[]int]int{})\n}\n",
        "map[[]int]int parameter",
    );
}

#[test]
fn slice_key_struct_field_rejects() {
    assert_rejects(
        "struct box {\n    m map[[]int]int\n}\n\nfn main() {\n    var b box\n    fmt.println(len(b.m))\n}\n",
        "map[[]int]int struct field",
    );
}

#[test]
fn func_key_rejects() {
    assert_rejects(
        "fn main() {\n    m := map[fn()]int{}\n    fmt.println(len(m))\n}\n",
        "map[fn()]int",
    );
}

#[test]
fn map_key_rejects() {
    assert_rejects(
        "fn main() {\n    m := map[map[string]int]int{}\n    fmt.println(len(m))\n}\n",
        "map[map[string]int]int",
    );
}

#[test]
fn array_of_slice_key_rejects() {
    assert_rejects(
        "fn main() {\n    m := map[[2][]int]int{}\n    fmt.println(len(m))\n}\n",
        "map[[2][]int]int",
    );
}

#[test]
fn struct_with_slice_key_rejects() {
    assert_rejects(
        "struct k {\n    xs []int\n}\n\nfn main() {\n    m := map[k]int{}\n    fmt.println(len(m))\n}\n",
        "map[k]int with a slice field",
    );
}

#[test]
fn nested_struct_with_slice_key_rejects() {
    assert_rejects(
        "struct inner {\n    xs []int\n}\n\nstruct outer {\n    i inner\n}\n\nfn main() {\n    m := map[outer]int{}\n    fmt.println(len(m))\n}\n",
        "map[outer]int with a transitively incomparable field",
    );
}

#[test]
fn alias_to_slice_key_rejects() {
    assert_rejects(
        "type idx = []int\n\nfn main() {\n    m := map[idx]int{}\n    fmt.println(len(m))\n}\n",
        "map[idx]int where idx aliases []int",
    );
}

// ---------------------------------------------------------------------------
// Controls — legal Go map keys that must stay accepted.
// ---------------------------------------------------------------------------

#[test]
fn any_chan_pointer_array_struct_keys_accept() {
    let src = r#"struct k {
    a int
    s string
}

fn main() {
    var u map[any]int
    fmt.println(len(u))
    var c map[chan int]int
    fmt.println(len(c))
    var p map[*k]int
    fmt.println(len(p))
    var a map[[2]int]int
    fmt.println(len(a))
    var s map[k]int
    fmt.println(len(s))
    var str map[string]int
    fmt.println(len(str))
}
"#;
    assert_accepts(src, "any/chan/pointer/array/comparable-struct/string keys");
}
