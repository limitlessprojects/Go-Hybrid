//! Additional regression tests for issue #1115 (`foreign.rs`):
//! - `serde_json.new_array` nested view borrows and `push_string` mutation
//!   conflicts (`HYB-E3101`) and borrow-handle escapes (`HYB-E3103`).
//! - `flate2.write`, `serde_json.set`/`remove`/`push_string`,
//!   `atomic.swap_*`/`store_uintptr`, and `fmt.scanf`/`sscanln` mutating
//!   `const` bindings in place (`HYB-E3006`).
//! - Unproven `atomic.value` and `atomic.pointer` results rejected across
//!   `task` spawns and `chan` sends (`HYB-E3005`).

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check_codes(src: &str) -> Vec<String> {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("unexpected parse errors: {:?}", diags));
    check_file(&ast)
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code)
        .collect()
}

#[test]
fn serde_json_new_array_nested_view_and_push_string_enforce_a2_and_a6() {
    let src = "\
struct box_view {
    v serde_json.view
}
fn main() {
    arr := serde_json.new_array()
    elem := arr.get(\"0\")?
    arr.push_string(\"x\")
    b := box_view{v: elem}
    fmt.println(elem.string(\"k\")?)
    _ = b
}
";
    let codes = check_codes(src);
    assert!(codes.iter().any(|c| c == "HYB-E3101"), "expected HYB-E3101, got {:?}", codes);
    assert!(codes.iter().any(|c| c == "HYB-E3103"), "expected HYB-E3103, got {:?}", codes);
}

#[test]
fn flate2_write_and_serde_json_and_fmt_mutators_reject_const_targets_hyb_e3006() {
    let src_serde = "\
const doc = serde_json.new_object()
fn main() {
    serde_json.set(doc, \"k\", \"v\")
}
";
    assert!(check_codes(src_serde).iter().any(|c| c == "HYB-E3006"));

    let src_remove = "\
const doc = serde_json.new_object()
fn main() {
    serde_json.remove(doc, \"k\")
}
";
    assert!(check_codes(src_remove).iter().any(|c| c == "HYB-E3006"));

    let src_push = "\
const arr = serde_json.new_array()
fn main() {
    serde_json.push_string(arr, \"v\")
}
";
    assert!(check_codes(src_push).iter().any(|c| c == "HYB-E3006"));

    let src_flate = "\
const w = flate2.new_writer(6)
fn main() {
    _ = flate2.write(w, [1, 2, 3])
}
";
    assert!(check_codes(src_flate).iter().any(|c| c == "HYB-E3006"));

    let src_scanf = "\
struct box_int { v int }
const fixed = box_int{v: 1}
fn main() {
    _, _ = fmt.scanf(\"%d\", &fixed.v)
}
";
    assert!(check_codes(src_scanf).iter().any(|c| c == "HYB-E3006"));

    let src_sscanln = "\
struct box_int { v int }
const fixed = box_int{v: 1}
fn main() {
    _, _ = fmt.sscanln(\"42\", &fixed.v)
}
";
    assert!(check_codes(src_sscanln).iter().any(|c| c == "HYB-E3006"));

    let src_atomic_swap = "\
struct slot_u32 { v uint32 }
const fixed = slot_u32{v: 0}
fn main() {
    _ = atomic.swap_uint32(&fixed.v, 9)
}
";
    assert!(check_codes(src_atomic_swap).iter().any(|c| c == "HYB-E3006"));
}

#[test]
fn unproven_atomic_value_and_pointer_reject_at_chan_and_task_boundaries_hyb_e3005() {
    let src_chan_val = "\
fn main() {
    ch := make(chan int, 1)
    v := atomic.value(1)
    ch <- v
}
";
    assert!(check_codes(src_chan_val).iter().any(|c| c == "HYB-E3005"));

    let src_task_ptr = "\
fn worker(x int) {}
fn main() {
    slot := 0
    p := atomic.pointer(&slot)
    task worker(p)
}
";
    assert!(check_codes(src_task_ptr).iter().any(|c| c == "HYB-E3005"));
}
