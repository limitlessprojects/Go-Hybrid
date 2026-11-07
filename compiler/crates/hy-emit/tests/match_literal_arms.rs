//! match_literal_arms.rs — literal-arm matches must lower to a Go `switch`.
//!
//! A `match` whose arms are literals (and `_`) passed `hyb check` but the
//! emitter refused it: `emit_match` classified only `some`/`none`/`_` and
//! bailed on `MatchPattern::Lit`, so `go build` never ran. Literal arms now
//! take a tagless `switch` path; `some`/`none` keep the option lowering.

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit_src(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parses");
    emit_go(&ast).expect("emits")
}

#[test]
fn int_literal_arms_emit_a_switch_with_a_default() {
    let go = emit_src(
        "\
fn describe(n int) string {
    match n {
        0: return \"zero\"
        1: return \"one\"
        _: return \"many\"
    }
}
",
    );
    assert!(go.contains("switch {"), "expected a switch, got:\n{go}");
    assert!(go.contains("== 0:"), "expected a case for 0, got:\n{go}");
    assert!(go.contains("== 1:"), "expected a case for 1, got:\n{go}");
    assert!(go.contains("default:"), "expected a default arm, got:\n{go}");
    assert!(
        !go.contains("hyMatch0.ok"),
        "literal arms must not take the option lowering:\n{go}"
    );
}

#[test]
fn string_literal_arms_emit_quoted_cases() {
    let go = emit_src(
        "\
fn f(s string) string {
    match s {
        \"x\": return \"ex\"
        _: return \"other\"
    }
}
",
    );
    assert!(go.contains("switch {"), "expected a switch, got:\n{go}");
    assert!(go.contains("== \"x\":"), "expected a quoted case, got:\n{go}");
}

#[test]
fn option_match_still_uses_the_option_lowering() {
    let go = emit_src(
        "\
fn f(o option[int]) string {
    match o {
        some(v): return \"{v}\"
        none: return \"none\"
    }
}
",
    );
    assert!(
        !go.contains("switch {"),
        "option match must keep the if/else lowering:\n{go}"
    );
    assert!(go.contains(".ok"), "expected the option `.ok` probe:\n{go}");
}
