//! Regression tests for `foreign.rs` soundness holes across M5/M6 curated packages:
//! 1. `serde_json.new_array` and `serde_json.push_string` omitted from `package_mode`,
//!    `package_result`, and `method_mode`, bypassing `O7`/`A2` (`HYB-E3101`) and `A6` (`HYB-E3103`).
//! 2. Curated in-place mutators (`hex.decode`, `json.unmarshal`, `json.compact`,
//!    `json.html_escape`, `json.indent`, `atomic.store_*`/`add_*`/`swap_*`/
//!    `compare_and_swap_*`, `fmt.scan`/`sscan`/`sscanf`) omitted from `package_mutated_arg`,
//!    bypassing `T22` `const` immutability (`HYB-E3006`).
//! 3. Unproven non-scalar `atomic.value` and `atomic.pointer` calls in `package_result`
//!    falling through to `_ if pkg == "atomic" => TypeShape::Primitive`, bypassing
//!    `GUARANTEES.md` Row 11 / `O4.9` (`HYB-E3005`) at `task` and `chan` boundaries.

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
fn serde_json_new_array_and_push_string_enforce_a2_and_a6_handle_loans() {
    let src_push_method = "\
fn main() {
    doc := serde_json.parse(`{\"items\":[\"a\"]}`)?
    v := doc.array(\"items\")?
    doc.push_string(\"b\")
    fmt.println(v.string(\"0\")?)
}
";
    assert!(check_codes(src_push_method).iter().any(|c| c == "HYB-E3101"));

    let src_push_pkg = "\
fn main() {
    doc := serde_json.parse(`{\"items\":[\"a\"]}`)?
    v := serde_json.get(doc, \"items\")?
    serde_json.push_string(doc, \"b\")
    fmt.println(serde_json.int(v, \"n\")?)
}
";
    assert!(check_codes(src_push_pkg).iter().any(|c| c == "HYB-E3101"));

    let src_new_array_a2 = "\
fn main() {
    arr := serde_json.new_array()
    v := arr.get(\"0\")?
    arr.set(\"0\", \"x\")
    fmt.println(v.string(\"name\")?)
}
";
    assert!(check_codes(src_new_array_a2).iter().any(|c| c == "HYB-E3101"));

    let src_new_array_a6 = "\
struct holder {
    v serde_json.view
}
fn main() {
    arr := serde_json.new_array()
    v := arr.get(\"0\")?
    h := holder{v: v}
    _ = h
}
";
    assert!(check_codes(src_new_array_a6).iter().any(|c| c == "HYB-E3103"));
}

#[test]
fn curated_hex_and_json_mutators_enforce_t22_const_immutability_hyb_e3006() {
    let src_hex = "\
import \"encoding/hex\"
const dst []byte = [0, 0, 0, 0]
fn main() {
    src := [102, 111, 111, 98]
    _, _ = hex.decode(dst, src)
}
";
    assert!(check_codes(src_hex).iter().any(|c| c == "HYB-E3006"));

    let src_json_unmarshal = "\
import \"encoding/json\"
struct cfg { port int }
const global_cfg = cfg{port: 8080}
fn main() {
    raw := [123, 125]
    _ = json.unmarshal(raw, &global_cfg)
}
";
    assert!(check_codes(src_json_unmarshal).iter().any(|c| c == "HYB-E3006"));

    let src_json_compact = "\
import \"encoding/json\"
const dst []byte = [0, 0]
fn main() {
    raw := [123, 125]
    _ = json.compact(dst, raw)
}
";
    assert!(check_codes(src_json_compact).iter().any(|c| c == "HYB-E3006"));

    let src_json_html_escape = "\
import \"encoding/json\"
const dst []byte = [0, 0]
fn main() {
    raw := [60, 62]
    json.html_escape(dst, raw)
}
";
    assert!(check_codes(src_json_html_escape).iter().any(|c| c == "HYB-E3006"));

    let src_json_indent = "\
import \"encoding/json\"
const dst []byte = [0, 0]
fn main() {
    raw := [123, 125]
    _ = json.indent(dst, raw, \"\", \"  \")
}
";
    assert!(check_codes(src_json_indent).iter().any(|c| c == "HYB-E3006"));
}

#[test]
fn curated_atomic_and_fmt_mutators_enforce_t22_const_immutability_hyb_e3006() {
    let src_atomic_store = "\
struct slot64 { v int64 }
const shared_slot = slot64{v: 0}
fn main() {
    atomic.store_int64(&shared_slot.v, 42)
}
";
    assert!(check_codes(src_atomic_store).iter().any(|c| c == "HYB-E3006"));

    let src_atomic_add = "\
struct slot32 { v int32 }
const shared_slot = slot32{v: 0}
fn main() {
    _ = atomic.add_int32(&shared_slot.v, 1)
}
";
    assert!(check_codes(src_atomic_add).iter().any(|c| c == "HYB-E3006"));

    let src_atomic_cas = "\
struct slot_u64 { v uint64 }
const shared_slot = slot_u64{v: 0}
fn main() {
    _ = atomic.compare_and_swap_uint64(&shared_slot.v, 0, 1)
}
";
    assert!(check_codes(src_atomic_cas).iter().any(|c| c == "HYB-E3006"));

    let src_fmt_sscan = "\
struct box_int { v int }
const fixed = box_int{v: 1}
fn main() {
    _, _ = fmt.sscan(\"42\", &fixed)
}
";
    assert!(check_codes(src_fmt_sscan).iter().any(|c| c == "HYB-E3006"));

    let src_fmt_sscanf = "\
struct box_int { v int }
const fixed = box_int{v: 1}
fn main() {
    _, _ = fmt.sscanf(\"42\", \"%d\", &fixed.v)
}
";
    assert!(check_codes(src_fmt_sscanf).iter().any(|c| c == "HYB-E3006"));

    let src_fmt_scan = "\
struct box_int { v int }
const fixed = box_int{v: 1}
fn main() {
    _, _ = fmt.scan(&fixed.v)
}
";
    assert!(check_codes(src_fmt_scan).iter().any(|c| c == "HYB-E3006"));
}

#[test]
fn unproven_atomic_value_and_pointer_calls_reject_at_spawn_and_send_hyb_e3005() {
    let src_val = "\
fn leak(v int) {}
fn main() {
    v := atomic.value(1)
    task leak(v)
}
";
    assert!(check_codes(src_val).iter().any(|c| c == "HYB-E3005"));

    let src_ptr = "\
fn main() {
    ch := make(chan int, 1)
    slot := 0
    p := atomic.pointer(&slot)
    ch <- p
}
";
    assert!(check_codes(src_ptr).iter().any(|c| c == "HYB-E3005"));
}
