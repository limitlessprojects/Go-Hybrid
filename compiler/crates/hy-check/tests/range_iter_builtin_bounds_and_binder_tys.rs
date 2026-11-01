//! Integration tests for builtin integer range bounds (`len`, `cap`, `copy`,
//! `min`, `max`) in `range_iter.rs` and `TypeExpr` preservation on `SelectRecv`,
//! `ForKind::Iterate`, and `ForKind::Range` binders in `lib.rs`.

use hy_check::check_file;
use hy_syntax::parse_file;

fn check_codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src)
        .unwrap_or_else(|diags| panic!("parse failed: {:?}", diags));
    check_file(&ast)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn range_bound_len_cap_copy_reject_non_int_counter() {
    let cases = [
        r#"
fn main() {
    xs := []int{1, 2, 3}
    var lo i32 = 0
    for i in lo..len(xs) {
        _ = i
    }
}
"#,
        r#"
fn main() {
    xs := []int{1, 2, 3}
    var lo i16 = 0
    for i in lo..cap(xs) {
        _ = i
    }
}
"#,
        r#"
fn main() {
    a := []int{1, 2}
    b := []int{3, 4}
    var lo i64 = 0
    for i in lo..copy(a, b) {
        _ = i
    }
}
"#,
    ];
    for src in cases {
        let codes = check_codes(src);
        assert!(
            codes.iter().any(|c| c == "HYB-E2009"),
            "expected HYB-E2009 for mismatched range bound, got {:?}\nsrc:\n{}",
            codes,
            src
        );
    }
}

#[test]
fn range_bound_untyped_lo_with_len_passes() {
    let src = r#"
fn main() {
    xs := []int{1, 2, 3}
    for i in 0..len(xs) {
        _ = i
    }
}
"#;
    let codes = check_codes(src);
    assert!(codes.is_empty(), "expected 0 diagnostics, got {:?}", codes);
}

#[test]
fn range_bound_min_max_typed_and_untyped() {
    // Typed arg (`i8`) fixes `min(a, b)` to `i8`, rejecting `0..min(a, b)` and `min(a, b)..300`.
    let reject_untyped_lo_vs_i8_hi = r#"
fn main() {
    var a i8 = 3
    var b i8 = 5
    for i in 0..min(a, b) {
        _ = i
    }
}
"#;
    assert!(
        check_codes(reject_untyped_lo_vs_i8_hi)
            .iter()
            .any(|c| c == "HYB-E2009"),
        "expected HYB-E2009 for 0..min(a, b) where a, b are i8"
    );

    let reject_i8_lo_overflow_const_hi = r#"
fn main() {
    var a i8 = 3
    var b i8 = 5
    for i in min(a, b)..300 {
        _ = i
    }
}
"#;
    assert!(
        check_codes(reject_i8_lo_overflow_const_hi)
            .iter()
            .any(|c| c == "HYB-E2009"),
        "expected HYB-E2009 for min(a, b)..300 where min(a, b) is i8"
    );

    // Untyped integer args default `min(1, 2)` / `max(1, 2)` to `int`, rejecting `i32` counter.
    let reject_i32_lo_vs_untyped_max = r#"
fn main() {
    var lo i32 = 0
    for i in lo..max(1, 2) {
        _ = i
    }
}
"#;
    assert!(
        check_codes(reject_i32_lo_vs_untyped_max)
            .iter()
            .any(|c| c == "HYB-E2009"),
        "expected HYB-E2009 for lo..max(1, 2) where lo is i32"
    );

    // Matching widths pass.
    let pass_matching = r#"
fn main() {
    var a i8 = 1
    var b i8 = 5
    for i in min(a, b)..10 {
        _ = i
    }
    for j in 0..min(1, 2) {
        _ = j
    }
}
"#;
    let codes = check_codes(pass_matching);
    assert!(codes.is_empty(), "expected 0 diagnostics, got {:?}", codes);
}

#[test]
fn binder_type_expr_preserved_for_select_recv_iterate_and_range() {
    // 1. SelectRecv binder preserves `i8` element TypeExpr and `bool` ok TypeExpr.
    let select_recv_src = r#"
fn main() {
    ch := make(chan i8, 1)
    select {
        v, ok := <-ch: {
            if ok {
                for i in v..300 {
                    _ = i
                }
            }
        }
        default: {}
    }
}
"#;
    assert!(
        check_codes(select_recv_src)
            .iter()
            .any(|c| c == "HYB-E2009"),
        "expected HYB-E2009 when select-recv binder `v` of type i8 is used in `v..300`"
    );

    // 2. ForKind::Iterate single-var and two-var over slice/map/chan preserve TypeExpr.
    let iterate_slice_src = r#"
fn main() {
    xs := []i16{1, 2}
    for x in xs {
        for i in x..70000 {
            _ = i
        }
    }
}
"#;
    assert!(
        check_codes(iterate_slice_src)
            .iter()
            .any(|c| c == "HYB-E2009"),
        "expected HYB-E2009 when slice element binder `x` of type i16 is used in `x..70000`"
    );

    let iterate_map_src = r#"
fn main() {
    m := map[i8]i16{1: 2}
    for k, v in m {
        for i in k..200 {
            _ = i
        }
        _ = v
    }
}
"#;
    assert!(
        check_codes(iterate_map_src)
            .iter()
            .any(|c| c == "HYB-E2009"),
        "expected HYB-E2009 when map key binder `k` of type i8 is used in `k..200`"
    );

    // 3. ForKind::Range counter binder preserves fixed-width integer TypeExpr in nested range.
    let nested_range_src = r#"
fn main() {
    var n i8 = 10
    for i in i8(0)..n {
        for j in i..300 {
            _ = j
        }
    }
}
"#;
    assert!(
        check_codes(nested_range_src)
            .iter()
            .any(|c| c == "HYB-E2009"),
        "expected HYB-E2009 when outer range binder `i` of type i8 is used in `i..300`"
    );
}
