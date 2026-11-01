//! assign_place_scalar_widths_2854.rs — issue #2854 (Row 13 BUILDABLE).
//!
//! `#2720` (`int` widths) and `#2776` (`float` widths) closed the concrete
//! numeric mismatch for the shapes they could resolve: a LOCAL target, whose
//! canonical spelling lives in the scalar side table (`bind_*` keyed by name),
//! and a declaration's initializer. `place_scalar` resolved only an `Ident`
//! place, so an assignment to a struct FIELD or to an INDEX place stayed
//! unchecked: `s.n = b32` (`n int64`) and `m["k"] = b32` (`map[string]int64`)
//! were accepted and then failed `go build`
//! (`cannot use b32 (variable of type int32) as int64 value in assignment`).
//!
//! These are assignments in exactly the sense of #2854 — single, and each pair
//! of a parallel assignment — so the same check now resolves a field through
//! the declared field type and an index through the declared container type.
//! Everything unresolvable (an alias, a type parameter, an unannotated
//! composite literal with no readable element type) stays fail-open, which the
//! accept side pins.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_with_e1002(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts_without_e1002(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E1002"),
        "must NOT fire HYB-E1002 for {:?}, got {:?}",
        src,
        cs
    );
}

const PAIR: &str = "struct pair {\n    a int64\n    b int32\n    c float64\n    d float32\n}\n";
const NESTED: &str =
    "struct inner {\n    a int64\n}\n\nstruct outer {\n    i inner\n}\n";
const DECLS: &str = "    var n64 int64 = 1\n    var n32 int32 = 2\n    var g64 float64 = 1.0\n    var g32 float32 = 2.0\n";
/// Reads every name `DECLS` binds: the checker's `declared and never used`
/// rule is also an `HYB-E1002`, so an accept case has to use what it declares.
const USES: &str = "    fmt.println(\"{n64} {n32} {g64} {g32}\")\n";

// ------------------------------------------------------------ reject (#2854):


#[test]
fn struct_field_assignment_rejects_e1002() {
    // `int64` and `float64` fields.
    rejects_with_e1002(&format!(
        "{PAIR}fn main() {{\n{DECLS}    var s pair\n    s.a = n32\n    fmt.println(\"{{s.a}}\")\n}}"
    ));
    rejects_with_e1002(&format!(
        "{PAIR}fn main() {{\n{DECLS}    var s pair\n    s.c = g32\n    fmt.println(\"{{s.c}}\")\n}}"
    ));
    // A pointer base resolves through the same field lookup.
    rejects_with_e1002(&format!(
        "{PAIR}fn main() {{\n{DECLS}    var s pair\n    p := &s\n    p.a = n32\n    fmt.println(\"{{p.a}}\")\n}}"
    ));
    // A nested field.
    rejects_with_e1002(&format!(
        "{NESTED}fn main() {{\n{DECLS}    var o outer\n    o.i.a = n32\n    fmt.println(\"{{o.i.a}}\")\n}}"
    ));
}

#[test]
fn parallel_assignment_through_fields_rejects_e1002() {
    rejects_with_e1002(&format!(
        "{PAIR}fn main() {{\n{DECLS}    var s pair\n    s.a, s.b = n32, n64\n    fmt.println(\"{{s.a}} {{s.b}}\")\n}}"
    ));
    rejects_with_e1002(&format!(
        "{PAIR}fn main() {{\n{DECLS}    var s pair\n    s.c, s.d = g32, g64\n    fmt.println(\"{{s.c}} {{s.d}}\")\n}}"
    ));
}

#[test]
fn index_assignment_rejects_e1002() {
    // Declared map/slice/array types.
    rejects_with_e1002(&format!(
        "fn main() {{\n{DECLS}    var m map[string]int64 = map[string]int64{{}}\n    m[\"k\"] = n32\n    fmt.println(\"{{m[\"k\"]}}\")\n}}"
    ));
    rejects_with_e1002(&format!(
        "fn main() {{\n{DECLS}    var xs [2]int64\n    xs[0] = n32\n    fmt.println(\"{{xs[0]}}\")\n}}"
    ));
    rejects_with_e1002(&format!(
        "fn main() {{\n{DECLS}    var fs []float64 = []float64{{1.0}}\n    fs[0] = g32\n    fmt.println(\"{{fs[0]}}\")\n}}"
    ));
    // The `m := map[string]int64{}` / `xs := []int64{1}` idiom: the declared
    // type is recovered from the composite literal.
    rejects_with_e1002(&format!(
        "fn main() {{\n{DECLS}    m := map[string]int64{{}}\n    m[\"k\"] = n32\n    fmt.println(\"{{m[\"k\"]}}\")\n}}"
    ));
    rejects_with_e1002(&format!(
        "fn main() {{\n{DECLS}    xs := []int64{{1}}\n    xs[0] = n32\n    fmt.println(\"{{xs[0]}}\")\n}}"
    ));
}

#[test]
fn parallel_assignment_through_an_index_rejects_e1002() {
    rejects_with_e1002(&format!(
        "fn main() {{\n{DECLS}    m := map[string]int64{{}}\n    m[\"k\"], m[\"j\"] = n32, n64\n    fmt.println(\"{{m[\"k\"]}} {{m[\"j\"]}}\")\n}}"
    ));
}

// ------------------------------------------------------------ accept:

#[test]
fn untyped_constants_still_adapt_into_fields_and_places() {
    // An untyped constant is assignable to any numeric type (`var n int64 = 1`
    // builds), so every one of these must stay accepted.
    accepts_without_e1002(&format!(
        "{PAIR}fn main() {{\n    var s pair\n    s.a = 1\n    s.c = 1.5\n    fmt.println(\"{{s.a}} {{s.c}}\")\n}}"
    ));
    accepts_without_e1002(
        "fn main() {\n    m := map[string]int64{}\n    m[\"k\"] = 1\n    xs := []float64{1.0}\n    xs[0] = 2\n    fmt.println(\"{m[\"k\"]} {xs[0]}\")\n}",
    );
}

#[test]
fn matching_widths_through_fields_and_places_stay_accepted() {
    accepts_without_e1002(&format!(
        "{PAIR}fn main() {{\n{DECLS}    var s pair\n    s.a = n64\n    s.b = n32\n    s.c = g64\n    s.d = g32\n{USES}    fmt.println(\"{{s.a}} {{s.b}} {{s.c}} {{s.d}}\")\n}}"
    ));
    accepts_without_e1002(&format!(
        "fn main() {{\n{DECLS}    m := map[string]int64{{}}\n    m[\"k\"] = n64\n    xs := []int64{{1}}\n    xs[0] = n64\n{USES}    fmt.println(\"{{m[\"k\"]}} {{xs[0]}}\")\n}}"
    ));
}

#[test]
fn aliases_and_unresolvable_fields_stay_fail_open() {
    // `byte` is `uint8`: the canonical spellings agree, so no mismatch.
    accepts_without_e1002(
        "struct holder {\n    u uint8\n}\n\nfn main() {\n    var h holder\n    var by byte = 1\n    h.u = by\n    fmt.println(\"{h.u}\")\n}",
    );
    // A field declared through an alias has no canonical spelling here, so the
    // comparison fails open rather than guessing.
    accepts_without_e1002(&format!(
        "type my_i64 = int64\n\nstruct a {{\n    n my_i64\n}}\n\nfn main() {{\n{DECLS}    var s a\n    s.n = n32\n{USES}    fmt.println(\"{{s.n}}\")\n}}"
    ));
}

#[test]
fn conversions_and_struct_values_stay_accepted() {
    accepts_without_e1002(&format!(
        "{PAIR}fn main() {{\n{DECLS}    var s pair\n    s.a = int64(n32)\n{USES}    fmt.println(\"{{s.a}}\")\n}}"
    ));
    accepts_without_e1002(&format!(
        "{NESTED}fn main() {{\n    var o outer\n    var i inner\n    o.i = i\n    fmt.println(\"{{o.i.a}}\")\n}}"
    ));
}

#[test]
fn local_target_control_unchanged() {
    // #2720/#2776 locals keep rejecting; the place resolution is additive.
    rejects_with_e1002(&format!(
        "fn main() {{\n{DECLS}    n64 = n32\n    g64 = g32\n    fmt.println(\"{{n64}} {{g64}}\")\n}}"
    ));
    accepts_without_e1002(&format!(
        "fn main() {{\n{DECLS}    n64 = n64\n    g32 = g32\n{USES}    fmt.println(\"{{n64}} {{g32}}\")\n}}"
    ));
}
