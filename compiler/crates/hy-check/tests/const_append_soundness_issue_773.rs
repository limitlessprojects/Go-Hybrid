use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check_codes(src: &str) -> Vec<String> {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("unexpected parse diagnostics: {:?}", diags));
    check_file(&ast)
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn repro1_interprocedural_append_into_const_slice_rejected_e3006() {
    let src = r#"package main
const primes = []int{2, 3, 5, 7}
fn overwrite_second(s []int) { _ = append(s[:1], 99) }
fn push_into_reslice(s []int) { _ = append(s, 88) }
fn main() {
    overwrite_second(primes)
    push_into_reslice(primes[:1])
}
"#;
    let codes = check_codes(src);
    let e3006_count = codes.iter().filter(|c| c.as_str() == "HYB-E3006").count();
    assert_eq!(e3006_count, 2, "expected 2 HYB-E3006 diagnostics, got: {:?}", codes);
}

#[test]
fn repro1b_method_and_closure_append_into_const_param_rejected_e3006() {
    let src = r#"package main
struct box_slice { items []int }
fn (b box_slice) append_via_method() { _ = append(b.items[:1], 77) }
const frozen_box = box_slice{items: []int{10, 20, 30}}
const primes = []int{2, 3, 5, 7}
fn main() {
    frozen_box.append_via_method()
    let f = fn(xs []int) { _ = append(xs[:1], 99) }
    f(primes)
}
"#;
    let codes = check_codes(src);
    let e3006_count = codes.iter().filter(|c| c.as_str() == "HYB-E3006").count();
    assert_eq!(e3006_count, 2, "expected 2 HYB-E3006 diagnostics, got: {:?}", codes);
}

#[test]
fn repro2_self_shadowing_bounded_reslice_append_rejected_e3006() {
    let src = r#"package main
const primes = []int{2, 3, 5, 7}
fn main() {
    primes := primes[:1]
    _ = append(primes, 99)
}
"#;
    let codes = check_codes(src);
    assert!(codes.iter().any(|c| c == "HYB-E3006"), "expected HYB-E3006, got: {:?}", codes);
}

#[test]
fn repro2b_closure_self_shadowing_bounded_reslice_append_rejected_e3006() {
    let src = r#"package main
const primes = []int{2, 3, 5, 7}
fn main() {
    let f = fn() {
        primes := primes[:1]
        _ = append(primes, 99)
    }
    f()
}
"#;
    let codes = check_codes(src);
    assert!(codes.iter().any(|c| c == "HYB-E3006"), "expected HYB-E3006, got: {:?}", codes);
}

#[test]
fn repro3_const_composites_holding_spare_capacity_slices_rejected_e3006() {
    let src = r#"package main
struct holder { xs []int }
const primes = []int{2, 3, 5, 7}
const h_reslice = holder{xs: primes[:1]}
const h_cap = holder{xs: make([]int, 1, 4)}
const grid_reslice = [][]int{primes[:1]}
fn main() {
    _ = append(h_reslice.xs, 99)
    _ = append(h_cap.xs, 77)
    _ = append(grid_reslice[0], 88)
}
"#;
    let codes = check_codes(src);
    let e3006_count = codes.iter().filter(|c| c.as_str() == "HYB-E3006").count();
    assert_eq!(e3006_count, 3, "expected 3 HYB-E3006 diagnostics, got: {:?}", codes);
}

#[test]
fn repro3b_local_const_composite_holding_bounded_reslice_rejected_e3006() {
    let src = r#"package main
struct holder { xs []int }
const primes = []int{2, 3, 5, 7}
fn main() {
    const local_h = holder{xs: primes[:1]}
    _ = append(local_h.xs, 99)
}
"#;
    let codes = check_codes(src);
    assert!(codes.iter().any(|c| c == "HYB-E3006"), "expected HYB-E3006, got: {:?}", codes);
}

#[test]
fn negative_control_pure_literal_full_cap_const_append_still_allowed() {
    let src = r#"package main
struct config { ports []int }
const primes = []int{2, 3, 5, 7}
const conf = config{ports: []int{80, 443}}
const grid = [][]int{[]int{1, 2}}
fn main() {
    _ = append(primes, 11)
    _ = append(primes[1:], 13)
    _ = append(conf.ports, 8080)
    _ = append(grid[0], 3)
}
"#;
    let codes = check_codes(src);
    assert!(codes.is_empty(), "expected 0 diagnostics, got: {:?}", codes);
}
