//! Issue #2386: O8 slot-loan key snapshot (`hyKeyN`) when `k` is modified by
//! a closure assignment, a `select` receive header, or a `match` arm binding.

fn emit(src: &str) -> String {
    let file = hy_syntax::parse_file("t.hy", src).expect("parse");
    hy_emit::emit_go(&file).unwrap_or_else(|e| panic!("emit failed: {}", e.message))
}

const PRELUDE: &str = "struct svc {\n    count int\n}\n\n";

#[test]
fn closure_assigning_key_forces_hykey_snapshot() {
    let go = emit(&format!(
        "{PRELUDE}fn main() {{\n    k := \"a\"\n    m := map[string]svc{{\"a\": svc{{count: 1}}, \"b\": svc{{count: 2}}}}\n    setk := fn() {{\n        k = \"b\"\n    }}\n    s := &m[k]\n    setk()\n    s.count = 50\n    fmt.println(k)\n    fmt.println(m[\"a\"].count)\n}}\n"
    ));
    assert!(go.contains("hyKey0 := k"), "{go}");
    assert!(go.contains("m[hyKey0] = s"), "{go}");
    assert!(!go.contains("m[k] = s"), "{go}");
}

#[test]
fn closure_shadowing_key_does_not_force_snapshot() {
    let go = emit(&format!(
        "{PRELUDE}fn main() {{\n    k := \"a\"\n    m := map[string]svc{{\"a\": svc{{count: 1}}}}\n    show := fn() {{\n        k := \"z\"\n        fmt.println(k)\n    }}\n    s := &m[k]\n    show()\n    s.count = 50\n    fmt.println(m[k].count)\n}}\n"
    ));
    assert!(!go.contains("hyKey"), "{go}");
    assert!(go.contains("m[k] = s"), "{go}");
}
