use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check_codes(src: &str) -> (Verdict, Vec<String>) {
    let f = parse_file("test.hy", src).expect("parse ok");
    let r = check_file(&f);
    let codes = r
        .diagnostics
        .iter()
        .map(|d| format!("{}: {}", d.code, d.message))
        .collect();
    (r.verdict, codes)
}

#[test]
fn bracketed_type_args_in_composite_lits_and_type_positions_reject_undeclared_types() {
    let (v1, codes1) = check_codes(
        r#"
struct box[t any] {
    v t
}

fn main() {
    s := set[undef_elem_ty]{}
    b := box[undef_box_ty]{v: 1}
    xs := []box[undef_slice_ty]{}
    m := map[string]box[undef_map_ty]{}
    var local_b box[undef_var_ty]
    _ = s
    _ = b
    _ = xs
    _ = m
    _ = local_b
}
"#,
    );
    assert_eq!(v1, Verdict::Reject, "expected Reject, got {:?}", codes1);
    let e1001_count = codes1.iter().filter(|c| c.starts_with("HYB-E1001")).count();
    assert_eq!(
        e1001_count, 5,
        "expected 5 HYB-E1001 diagnostics for undeclared generic type args, got {:?}",
        codes1
    );
}

#[test]
fn generic_struct_and_fn_accept_own_type_params_and_reject_undeclared_type_args() {
    let (v_ok, codes_ok) = check_codes(
        r#"
struct box[t any] {
    v t
}

struct holder[t any] {
    inner box[t]
}

fn id_val[t any](x t) t {
    return x
}

fn main() {
    b := box[int]{v: 7}
    _ = id_val[int](b.v)
}
"#,
    );
    assert_eq!(v_ok, Verdict::Accept, "expected Accept, got {:?}", codes_ok);

    let (v_bad, codes_bad) = check_codes(
        r#"
struct box[t any] {
    v t
}

struct holder[t any] {
    inner box[undef_field_ty]
}

enum maybe {
    some_val(box[undef_payload_ty]),
    empty,
}

fn main() {}
"#,
    );
    assert_eq!(v_bad, Verdict::Reject, "expected Reject, got {:?}", codes_bad);
    let e1001_count = codes_bad.iter().filter(|c| c.starts_with("HYB-E1001")).count();
    assert_eq!(
        e1001_count, 2,
        "expected 2 HYB-E1001 diagnostics, got {:?}",
        codes_bad
    );
}

#[test]
fn match_two_segment_path_rejects_undeclared_enum_or_variant() {
    let (v1, codes1) = check_codes(
        r#"
enum color {
    red,
    green,
}

fn main() {
    c := red
    match c {
        undef_enum.red => {}
        color.undef_variant => {}
    }
}
"#,
    );
    assert_eq!(v1, Verdict::Reject, "expected Reject, got {:?}", codes1);
    let e1001_count = codes1.iter().filter(|c| c.starts_with("HYB-E1001")).count();
    assert_eq!(
        e1001_count, 2,
        "expected 2 HYB-E1001 diagnostics on two-segment match patterns, got {:?}",
        codes1
    );
}

#[test]
fn match_two_segment_path_accepts_declared_enum_and_option_surface() {
    let (v, codes) = check_codes(
        r#"
enum result_state {
    ready(int),
    waiting,
}

fn check_opt(x option[int], s result_state) int {
    total := 0
    match x {
        some(v) => {
            total = total + v
        }
        none => {}
    }
    match s {
        result_state.ready(n) => {
            total = total + n
        }
        result_state.waiting => {}
    }
    return total
}

fn main() {
    _ = check_opt(some(3), ready(4))
}
"#,
    );
    assert_eq!(v, Verdict::Accept, "expected Accept, got {:?}", codes);
}
