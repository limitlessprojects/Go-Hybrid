//! Regression tests for issue #1046: projected map-element borrows
//! (`&m[k].f`, `&m[k][i]`, `&m[k].arr[i]`) in argument and expression
//! positions must reject with HYB-E3104, while slice and pointer hops inside
//! map elements remain accepted.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
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

fn assert_e3104(src: &str, what: &str) {
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3104"),
        "{}: expected HYB-E3104, got {:?}",
        what,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "{}", what);
}

#[test]
fn map_element_field_borrow_as_call_arg_is_e3104() {
    assert_e3104(
        "struct acct { bal int }\n\
         fn touch(p *int) { *p = 9 }\n\
         fn main() {\n\
             m := map[string]acct{\"a\": acct{bal: 1}}\n\
             touch(&m[\"a\"].bal)\n\
             fmt.println(\"{m[\"a\"].bal}\")\n\
         }\n",
        "&m[\"a\"].bal in call argument position",
    );
}

#[test]
fn map_element_array_index_borrow_as_call_arg_is_e3104() {
    assert_e3104(
        "fn touch(p *int) { *p = 9 }\n\
         fn main() {\n\
             m := map[string][2]int{\"a\": [2]int{1, 2}}\n\
             touch(&m[\"a\"][0])\n\
             fmt.println(\"{m[\"a\"][0]}\")\n\
         }\n",
        "&m[\"a\"][0] on map of fixed-size arrays in call argument position",
    );
}

#[test]
fn map_element_nested_struct_array_borrow_as_call_arg_is_e3104() {
    assert_e3104(
        "struct bucket { slots [2]int }\n\
         fn touch(p *int) { *p = 9 }\n\
         fn main() {\n\
             m := map[string]bucket{\"a\": bucket{slots: [2]int{1, 2}}}\n\
             touch(&m[\"a\"].slots[0])\n\
             fmt.println(\"{m[\"a\"].slots[0]}\")\n\
         }\n",
        "&m[\"a\"].slots[0] in call argument position",
    );
}

#[test]
fn map_element_slice_hop_borrow_remains_accepted() {
    let src = "\
struct bucket { items []int }
fn touch(p *int) { *p = 9 }
fn main() {
    m := map[string]bucket{\"a\": bucket{items: []int{1, 2}}}
    touch(&m[\"a\"].items[0])
    fmt.println(\"{m[\"a\"].items[0]}\")
}
";
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "slice element inside map value is addressable in Go, got {:?}",
        r.diagnostics
    );
}
