//! emit_guard_overflow_shadow_and_generic.rs — regression tests for issue #3140: HYB-E6002
//! constant overflow checks across shadowed bindings, generic struct literals,
//! cyclic struct type aliases, and aliased function-type callees.

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn shadowed_file_const_in_return_composite_and_call_is_accepted() {
    let src = r#"const k = 300

struct box {
    v u8
}

fn sink(x u8) {
    _ = x
}

fn pass_through(k u8) u8 {
    b := box{v: k}
    xs := []u8{k}
    m := map[u8]u8{k: k}
    sink(k)
    _ = b
    _ = xs
    _ = m
    return k
}

fn main() {
    _ = pass_through(7)
}
"#;
    let found = codes(src);
    assert!(
        found.is_empty(),
        "local/parameter `k u8` shadowing file-level `const k = 300` must not trigger HYB-E6002, got {found:?}"
    );
}

#[test]
fn generic_struct_literal_and_aliased_fn_call_reject_overflowing_constants() {
    for (label, src) in [
        (
            "generic struct literal field overflow",
            "struct box[t any] {\n    v t\n}\n\nfn main() {\n    b := box[u8]{v: 300}\n    _ = b\n}\n",
        ),
        (
            "aliased generic struct literal field overflow",
            "struct box[t any] {\n    v t\n}\n\ntype byte_box = box[u8]\n\nfn main() {\n    b := byte_box{v: -1}\n    _ = b\n}\n",
        ),
        (
            "aliased fn-type parameter call overflow",
            "type byte_sink = fn(x u8)\n\nfn run(f byte_sink) {\n    f(300)\n}\n\nfn main() {}\n",
        ),
    ] {
        let found = codes(src);
        assert!(
            found.iter().any(|c| c == "HYB-E6002"),
            "{label}: expected HYB-E6002, got {found:?}"
        );
    }
}

#[test]
fn cyclic_type_alias_struct_literal_terminates_without_hang() {
    let src = "type loop_ty = loop_ty\n\nfn main() {\n    _ = loop_ty{}\n}\n";
    let _ = codes(src);
}
