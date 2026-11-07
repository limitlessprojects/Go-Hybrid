//! Regression tests for `any`-field composite sharing (`HYB-E3004` / `HYB-E3003`)
//! and closure-alias / slot-loan-alias A3 window extension (`HYB-E3102`).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src)
        .unwrap_or_else(|diags| panic!("expected source to parse cleanly, got: {:?}", diags));
    check_file(&ast)
}

fn error_codes(res: &hy_check::CheckResult) -> Vec<String> {
    res.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn any_param_embedded_in_struct_or_slice_rejects_move_e3004() {
    let spawn_struct = check(
        r#"
struct box {
    f any
}

fn worker(b box) {
    _ = b
}

fn leak(p any) {
    s := box{f: p}
    task worker(move s)
}
"#,
    );
    assert_eq!(
        spawn_struct.verdict,
        Verdict::Reject,
        "diags: {:?}",
        spawn_struct.diagnostics
    );
    assert!(
        error_codes(&spawn_struct).contains(&"HYB-E3004".to_string()),
        "expected HYB-E3004 when moving struct embedding `any` parameter `p`, got: {:?}",
        spawn_struct.diagnostics
    );

    let send_copied_box = check(
        r#"
struct box {
    f any
}

fn leak_send(p any, ch chan box) {
    s := box{f: p}
    s2 := s
    ch <- move s2
}
"#,
    );
    assert_eq!(
        send_copied_box.verdict,
        Verdict::Reject,
        "diags: {:?}",
        send_copied_box.diagnostics
    );
    assert!(
        error_codes(&send_copied_box).contains(&"HYB-E3004".to_string()),
        "expected HYB-E3004 when moving copied struct `s2` embedding `any` parameter `p`, got: {:?}",
        send_copied_box.diagnostics
    );
}

#[test]
fn copied_any_field_struct_preserves_map_freeze_e3003_and_scalar_control_accepts() {
    let freeze_via_copy = check(
        r#"
struct box {
    f any
}

fn worker(b box) {
    _ = b
}

fn main() {
    m := map[string]int{"k": 1}
    s := box{f: m}
    s2 := s
    task worker(move s2)
    m["k"] = 2
}
"#,
    );
    assert_eq!(
        freeze_via_copy.verdict,
        Verdict::Reject,
        "diags: {:?}",
        freeze_via_copy.diagnostics
    );
    assert!(
        error_codes(&freeze_via_copy).contains(&"HYB-E3003".to_string()),
        "expected HYB-E3003 when mutating `m` after moving `s2 := s` (`box{{f: m}}`), got: {:?}",
        freeze_via_copy.diagnostics
    );

    let scalar_ok = check(
        r#"
struct scalar {
    x int
}

fn worker(s scalar) {
    _ = s
}

fn main() {
    s := scalar{x: 1}
    s2 := s
    task worker(move s2)
    s.x = 2
}
"#,
    );
    assert_eq!(
        scalar_ok.verdict,
        Verdict::Accept,
        "diags: {:?}",
        scalar_ok.diagnostics
    );
}

#[test]
fn closure_capturing_slot_loan_alias_or_copied_closure_extends_a3_window_e3102() {
    let slot_alias_in_closure = check(
        r#"
struct svc {
    count int
}

fn main() {
    m := map[string]svc{"a": svc{count: 1}}
    s := &m["a"]
    p := s
    f := fn() {
        p.count = 99
    }
    m["b"] = svc{count: 2}
    f()
}
"#,
    );
    assert_eq!(
        slot_alias_in_closure.verdict,
        Verdict::Reject,
        "diags: {:?}",
        slot_alias_in_closure.diagnostics
    );
    assert!(
        error_codes(&slot_alias_in_closure).contains(&"HYB-E3102".to_string()),
        "expected HYB-E3102 when closure captures slot-loan alias `p := s` across map insert, got: {:?}",
        slot_alias_in_closure.diagnostics
    );

    let copied_closure = check(
        r#"
struct svc {
    count int
}

fn main() {
    m := map[string]svc{"a": svc{count: 1}}
    s := &m["a"]
    f := fn() {
        s.count = 99
    }
    f2 := f
    m["b"] = svc{count: 2}
    f2()
}
"#,
    );
    assert_eq!(
        copied_closure.verdict,
        Verdict::Reject,
        "diags: {:?}",
        copied_closure.diagnostics
    );
    assert!(
        error_codes(&copied_closure).contains(&"HYB-E3102".to_string()),
        "expected HYB-E3102 when copied closure `f2 := f` is called after map insert, got: {:?}",
        copied_closure.diagnostics
    );
}
