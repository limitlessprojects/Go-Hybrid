//! range_element_mutation_1480.rs — regression for #1480 (silent range-element
//! mutation drop).
//!
//! `for x in xs` lowers to Go's `for _, x := range xs`, so a struct/array
//! element binding is a per-iteration copy. `x.n += 10`, `x.f = v`,
//! `x[i] = v` and auto-addressed pointer-receiver calls through such a
//! binding all built and ran clean while the collection never saw the write —
//! a misleading pass one step away from the O8 map-slot semantics the corpus
//! pins as working. hy-check now rejects the dropped writes (rule: BUILDABLE,
//! HYB-E1002) instead of certifying them; writes that reach shared storage
//! (pointer elements, slice/map element stores through the collection) and
//! plain reads must keep accepting.

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

const CELL: &str = "struct cell {\n    n int\n}\n";
const CELL_PTR_BUMP: &str =
    "struct cell {\n    n int\n}\n\nfn (c *cell) bump() {\n    c.n += 10\n}\n";
const CELL_VAL_READ: &str =
    "struct cell {\n    n int\n}\n\nfn (c cell) read() int {\n    return c.n\n}\n";

// ---------------------------------------------------------------- reject:

#[test]
fn field_compound_assign_through_single_range_var_rejects() {
    // Form A of the issue.
    rejects_with_e1002(&format!(
        "{}fn main() {{\n    xs := []cell{{cell{{n: 1}}, cell{{n: 2}}}}\n    for x in xs {{\n        x.n += 10\n    }}\n    fmt.println(\"{{xs[0].n}}\")\n}}",
        CELL
    ));
}

#[test]
fn field_plain_assign_through_single_range_var_rejects() {
    rejects_with_e1002(&format!(
        "{}fn main() {{\n    xs := []cell{{cell{{n: 1}}}}\n    for x in xs {{\n        x.n = 5\n    }}\n    fmt.println(\"{{xs[0].n}}\")\n}}",
        CELL
    ));
}

#[test]
fn field_write_through_two_var_range_rejects() {
    // Form B: the value binding of `for i, x in xs` is the copy.
    rejects_with_e1002(&format!(
        "{}fn main() {{\n    xs := []cell{{cell{{n: 1}}}}\n    for i, x in xs {{\n        x.n += 10\n        _ = i\n    }}\n    fmt.println(\"{{xs[0].n}}\")\n}}",
        CELL
    ));
}

#[test]
fn index_var_is_not_the_copy_and_writing_through_collection_rejects_nothing() {
    // `for i, x in xs { xs[i].n = 1 }` writes the collection, not the copy.
    accepts_without_e1002(&format!(
        "{}fn main() {{\n    xs := []cell{{cell{{n: 1}}}}\n    for i, x in xs {{\n        xs[i].n = 1\n        _ = x\n    }}\n    fmt.println(\"{{xs[0].n}}\")\n}}",
        CELL
    ));
}

#[test]
fn pointer_receiver_call_on_range_element_rejects() {
    // Form C: the element IS addressable, so #656's non-addressable rule
    // passes it; Go builds `(&x).Bump()` and the mutation is dropped.
    rejects_with_e1002(&format!(
        "{}fn main() {{\n    xs := []cell{{cell{{n: 1}}}}\n    for x in xs {{\n        x.bump()\n    }}\n    fmt.println(\"{{xs[0].n}}\")\n}}",
        CELL_PTR_BUMP
    ));
}

#[test]
fn nested_field_chain_through_copy_rejects() {
    let src = "struct inner {\n    k int\n}\n\nstruct outer {\n    deep inner\n}\n\nfn main() {\n    os := []outer{outer{deep: inner{k: 1}}}\n    for o in os {\n        o.deep.k = 9\n    }\n    fmt.println(\"{os[0].deep.k}\")\n}";
    rejects_with_e1002(src);
}

#[test]
fn array_element_row_write_through_range_rejects() {
    // `matrix` is a slice of fixed arrays; `row` is an Array kind, and
    // `row[0] = 9` mutates the per-iteration copy of the array.
    let src = "fn main() {\n    matrix := [][2]int{[2]int{1, 2}, [2]int{3, 4}}\n    for row in matrix {\n        row[0] = 9\n    }\n    fmt.println(\"{matrix[0][0]}\")\n}";
    rejects_with_e1002(src);
}

#[test]
fn map_value_struct_write_through_range_rejects() {
    // `for k, v in m` binds v as a copy of the map value.
    rejects_with_e1002(&format!(
        "{}fn main() {{\n    m := map[string]cell{{\"a\": cell{{n: 1}}}}\n    for k, v in m {{\n        v.n = 7\n        _ = k\n    }}\n    fmt.println(m[\"a\"].n)\n}}",
        CELL
    ));
}

#[test]
fn single_var_map_range_binds_the_key_copy_and_rejects() {
    // SPEC-LANGUAGE LX13: one var over a map is the KEY. If the key is a struct,
    // it binds as a copy of the key and mutating its fields rejects with HYB-E1002.
    rejects_with_e1002(&format!(
        "{}fn main() {{\n    m := map[cell]int{{cell{{n: 1}}: 1}}\n    for k in m {{\n        k.n = 7\n    }}\n    fmt.println(len(m))\n}}",
        CELL
    ));
}

// ---------------------------------------------------------------- accept:

#[test]
fn pointer_element_collection_writes_stay_accepted() {
    // []*cell: the element is a pointer, mutations reach shared storage.
    accepts_without_e1002(&format!(
        "{}fn main() {{\n    ps := []*cell{{&cell{{n: 1}}, &cell{{n: 2}}}}\n    for p in ps {{\n        p.n += 5\n        p.bump()\n    }}\n    fmt.println(\"{{ps[0].n}} {{ps[1].n}}\")\n}}",
        CELL_PTR_BUMP
    ));
}

#[test]
fn collection_indexed_write_stays_accepted() {
    // The fix-it shape itself must build.
    accepts_without_e1002(&format!(
        "{}fn main() {{\n    xs := []cell{{cell{{n: 1}}}}\n    for i in 0..len(xs) {{\n        xs[i].n += 10\n    }}\n    fmt.println(\"{{xs[0].n}}\")\n}}",
        CELL
    ));
}

#[test]
fn plain_local_struct_write_stays_accepted() {
    accepts_without_e1002(&format!(
        "{}fn main() {{\n    var z cell\n    z.n = 3\n    fmt.println(\"{{z.n}}\")\n}}",
        CELL
    ));
}

#[test]
fn slice_field_element_write_through_copy_stays_accepted() {
    // `w` is a copy, but `w.items` shares the backing array with the stored
    // element — `w.items[0] = 1` is visible through the collection and must
    // NOT be swept up by the rule.
    let src = "struct wrap {\n    items []int\n}\n\nfn main() {\n    ws := []wrap{wrap{items: []int{1, 2}}}\n    for w in ws {\n        w.items[0] = 1\n    }\n    fmt.println(\"{ws[0].items[0]}\")\n}";
    accepts_without_e1002(src);
}

#[test]
fn range_element_field_reads_stay_accepted() {
    accepts_without_e1002(&format!(
        "{}fn main() {{\n    xs := []cell{{cell{{n: 1}}}}\n    total := 0\n    for x in xs {{\n        total += x.n\n    }}\n    fmt.println(\"{{total}}\")\n}}",
        CELL
    ));
}

#[test]
fn value_receiver_method_call_on_range_element_stays_accepted() {
    // A value receiver cannot mutate the element — no silent drop.
    accepts_without_e1002(&format!(
        "{}fn main() {{\n    xs := []cell{{cell{{n: 1}}}}\n    for x in xs {{\n        v := x.read()\n        fmt.println(\"{{v}}\")\n    }}\n}}",
        CELL_VAL_READ
    ));
}

#[test]
fn scalar_element_loop_is_unaffected() {
    accepts_without_e1002("fn main() {\n    ints := []int{1, 2}\n    total := 0\n    for n in ints {\n        total += n\n    }\n    fmt.println(\"{total}\")\n}");
}

#[test]
fn whole_element_reassignment_stays_accepted() {
    // `x = cell{...}` stores into the loop-local binding; it is the one
    // target form Go (and this checker) treats as an ordinary assignment.
    accepts_without_e1002(&format!(
        "{}fn main() {{\n    xs := []cell{{cell{{n: 1}}}}\n    for x in xs {{\n        x = cell{{n: 0}}\n        fmt.println(\"{{x.n}}\")\n    }}\n}}",
        CELL
    ));
}

#[test]
fn outer_variable_with_same_name_is_not_marked_inside_shadowing_loop() {
    // The copy marking is lexical: writes to `x` outside the loop body,
    // under a name the loop shadows, must not inherit the reject.
    accepts_without_e1002(&format!(
        "{}fn main() {{\n    var x cell\n    x.n = 1\n    xs := []cell{{cell{{n: 2}}}}\n    for x in xs {{\n        _ = x.n\n    }}\n    fmt.println(\"{{x.n}}\")\n}}",
        CELL
    ));
}
