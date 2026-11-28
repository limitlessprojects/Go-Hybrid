//! Regression (#3933): `hy-check` only fails closed on phantom `time.time`
//! methods when the emitter can actually LOWER the accepted ones — the
//! checker surface must be a subset of this member table (checker ⊆ emitter ⊆
//! Go, GUARANTEES row 13). These pins are the emitter half of that contract:
//! the 28 single-return value methods spell their exact `time.Time` Go names,
//! and chains like `s.t.utc().unix()` resolve through the result descriptors.
//!
//! Surface spelling: `time.time` methods are CALLED in hy (`s.t.totally_fake()`
//! in the issue repro, `utc().unix()` in its pins), so every entry is a plain
//! `method` — same as sync.mutex's zero-arg `lock`. `method0` is the
//! property-style surface (`dt.milliseconds`, no parens in hy) and must NOT
//! be used here: a parenthesized call against a `method0` entry falls through
//! emit_call and doubles the parens (`Year()()`).

use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn time_time_value_methods_emit_exact_go_spellings() {
    let src = r#"struct stats {
    t time.time
    d time.duration
    n int
}
fn main() {
    var s stats
    y := s.t.year()
    ts := s.t.utc().unix()
    txt := s.t.format("2006-01-02")
    later := s.t.add(s.d)
    ns := s.t.sub(s.d).nanoseconds
    fmt.println("y={y} ts={ts} txt={txt} ns={ns} n={s.n}")
}
"#;
    let f = parse_file("test.hy", src).expect("parse");
    let go = emit_go(&f).expect("emit");
    assert!(go.contains("s.t.Year()"), "{go}");
    assert!(go.contains("s.t.UTC().Unix()"), "{go}");
    assert!(go.contains("s.t.Format(\"2006-01-02\")"), "{go}");
    assert!(go.contains("s.t.Add(s.d)"), "{go}");
    assert!(go.contains(".Sub(s.d).Nanoseconds()"), "{go}");
    assert!(!go.contains("()()"), "double-paren emission:\n{go}");
}

#[test]
fn time_time_zero_arg_methods_emit_one_paren_pair() {
    // `Year`/`Unix`/`IsZero` are Go METHODS called with parens in hy: the
    // emitted Go carries exactly one call-paren pair per call.
    let src = r#"struct stats {
    t time.time
    n int
}
fn main() {
    var s stats
    z := s.t.is_zero()
    us := s.t.local().unix_micro()
    fmt.println("z={z} us={us} n={s.n}")
}
"#;
    let f = parse_file("test.hy", src).expect("parse");
    let go = emit_go(&f).expect("emit");
    assert!(go.contains("s.t.IsZero()"), "{go}");
    assert!(go.contains("s.t.Local().UnixMicro()"), "{go}");
    assert!(!go.contains("()()"), "double-paren emission:\n{go}");
}
