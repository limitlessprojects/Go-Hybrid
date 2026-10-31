//! Issue #2386 (closure variant): an O8 slot loan `s := &m[k]` stores back
//! through `m[k]` at the end of its window. When a closure assigns `k` and
//! runs inside that window, the store-back used the NEW key — the program
//! passed `hyb check` and silently overwrote the wrong map entry. The
//! statement scan (`stmt_modifies_ident`) cannot see a write hidden behind
//! a call, so any key name a closure literal assigns is snapshotted.

fn emit(src: &str) -> String {
    let file = hy_syntax::parse_file("t.hy", src).expect("parse");
    hy_emit::emit_go(&file).unwrap_or_else(|e| panic!("emit failed: {}", e.message))
}

const PRELUDE: &str = "struct svc {\n    count int\n}\n\n";

#[test]
fn closure_defined_before_the_loan_forces_a_key_snapshot() {
    let go = emit(&format!(
        "{PRELUDE}fn main() {{\n    k := \"a\"\n    m := map[string]svc{{\"a\": svc{{count: 1}}, \"b\": svc{{count: 2}}}}\n    setk := fn() {{\n        k = \"b\"\n    }}\n    s := &m[k]\n    setk()\n    s.count = 50\n    fmt.println(k)\n    fmt.println(m[\"a\"].count)\n}}\n"
    ));
    assert!(go.contains("hyKey0 := k"), "{go}");
    assert!(go.contains("m[hyKey0] = s"), "{go}");
    assert!(!go.contains("m[k] = s"), "{go}");
}

#[test]
fn closure_defined_inside_the_loan_window_forces_a_key_snapshot() {
    let go = emit(&format!(
        "{PRELUDE}fn main() {{\n    k := \"a\"\n    m := map[string]svc{{\"a\": svc{{count: 1}}, \"b\": svc{{count: 2}}}}\n    s := &m[k]\n    setk := fn() {{\n        if true {{\n            k = \"b\"\n        }}\n    }}\n    setk()\n    s.count = 50\n    fmt.println(k)\n    fmt.println(m[\"a\"].count)\n}}\n"
    ));
    assert!(go.contains("m[hyKey0] = s"), "{go}");
    assert!(!go.contains("m[k] = s"), "{go}");
}

#[test]
fn closure_shadowing_the_key_is_not_a_write() {
    // `k := …` inside the closure is a fresh binding — the outer key is
    // untouched, so the emission shape stays the plain `m[k]` store-back.
    let go = emit(&format!(
        "{PRELUDE}fn main() {{\n    k := \"a\"\n    m := map[string]svc{{\"a\": svc{{count: 1}}}}\n    show := fn() {{\n        k := \"z\"\n        fmt.println(k)\n    }}\n    s := &m[k]\n    show()\n    s.count = 50\n    fmt.println(m[k].count)\n}}\n"
    ));
    assert!(!go.contains("hyKey"), "{go}");
    assert!(go.contains("m[k] = s"), "{go}");
}
