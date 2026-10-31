//! Match-arm bodies that redeclare the arm's pattern binding.
//!
//! hy-check treats a match arm as a nested scope, so
//! `some(v): { v := v + 1 ... }` is a legal arm-local shadow. The emitter
//! injects `v := hyMatch0.val` into the same Go block as the inlined arm
//! body, though, so the body's `v := v + 1` used to produce
//! `no new variables on left side of :=` in Go. The fix keeps the injection
//! in the arm's Go scope and wraps ONLY a redefining arm body in a nested
//! block; arms that do not redefine the binding keep byte-identical output.

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let ast = parse_file("match_arm_redeclare.hy", src).expect("parses");
    emit_go(&ast).expect("emits")
}

#[test]
fn redefining_unguarded_arm_body_gets_nested_scope_and_keeps_binding_inject() {
    // The `some(v)` arm redeclares `v` at its top level after reading it:
    // the pattern binding must stay injected in the arm's Go scope (`v :=
    // hyMatch0.val` outside the wrap), and the arm body must land in a
    // nested block where `v := v + 1` is a fresh legal declaration.
    let go = emit(
        "fn main() {\n    o := some(2)\n    match o {\n        some(v): { v := v + 1\n            fmt.println(v) }\n        none: fmt.println(0)\n    }\n}\n",
    );
    let inject_pos = go.find("\t\tv := hyMatch0.val\n").unwrap_or_else(|| {
        panic!("expected the pattern binding injection, got:\n{go}")
    });
    let wrap_pos = go.find("\t\t{\n").unwrap_or_else(|| {
        panic!("expected a nested block wrapping the redefining arm body, got:\n{go}")
    });
    let body_pos = go.find("\t\t\tv := v + 1\n").unwrap_or_else(|| {
        panic!(
            "expected the arm shadow to be a fresh declaration inside the nested block, got:\n{go}"
        )
    });
    let close_pos = go.find("\t\t}\n").unwrap_or_else(|| {
        panic!("expected the nested wrap to close, got:\n{go}")
    });
    assert!(
        inject_pos < wrap_pos && wrap_pos < body_pos && body_pos < close_pos,
        "expected inject -> nested block -> shadow -> close, got:\n{go}"
    );
}

#[test]
fn redefining_arm_in_multi_arm_match_gets_the_same_treatment() {
    // Three reachable arms route through the has_unguarded_some lowering —
    // the per-arm block there must nest the redefining body the same way.
    let go = emit(
        "fn main() {\n    o := some(3)\n    match o {\n        some(v) if v > 0: fmt.println(v)\n        some(w): { w := w + 1\n            fmt.println(w) }\n        none: fmt.println(0)\n    }\n}\n",
    );
    let inject_pos = go.find("\t\t\tw := hyMatch0.val\n").unwrap_or_else(|| {
        panic!("expected the `w` pattern binding injection, got:\n{go}")
    });
    let wrap_pos = go.find("\t\t\t{\n").unwrap_or_else(|| {
        panic!("expected a nested block wrapping the redefining arm body, got:\n{go}")
    });
    let body_pos = go.find("\t\t\t\tw := w + 1\n").unwrap_or_else(|| {
        panic!(
            "expected the arm shadow to be a fresh declaration inside the nested block, got:\n{go}"
        )
    });
    assert!(
        inject_pos < wrap_pos && wrap_pos < body_pos,
        "expected inject -> nested block -> shadow, got:\n{go}"
    );
}

#[test]
fn arm_body_redefines_without_reading_keeps_no_injection_and_no_wrap() {
    // `some(v): { v := 5 ... }` never reads the pre-shadow binding, so the
    // shadow-aware body_reads gate skips the injection entirely (an injected
    // but unread `v` would trip Go's `declared and not used`) and the plain
    // arm body is already a legal fresh declaration.
    let go = emit(
        "fn main() {\n    o := some(9)\n    match o {\n        some(v): { v := 5\n            fmt.println(v) }\n        none: fmt.println(0)\n    }\n}\n",
    );
    assert!(
        !go.contains("v := hyMatch0.val"),
        "an unread pattern binding must not be injected, got:\n{go}"
    );
    assert!(
        go.contains("\t\tv := 5\n"),
        "expected the shadow define to be emitted inline, got:\n{go}"
    );
    assert!(
        !go.contains("\t\t{\n"),
        "an arm body without an injected binding must not gain a wrap, got:\n{go}"
    );
}

#[test]
fn ordinary_unguarded_arm_output_stays_flat() {
    // Arms that do not redefine the pattern binding keep the pre-fix shape
    // exactly: inject, then the arm body inline in the same block — no
    // extra nesting (byte-exact output for every non-redefining program).
    let go = emit(
        "fn main() {\n    o := some(2)\n    match o {\n        some(v): fmt.println(v)\n        none: fmt.println(0)\n    }\n}\n",
    );
    let inject_pos = go.find("\t\tv := hyMatch0.val\n").unwrap_or_else(|| {
        panic!("expected the pattern binding injection, got:\n{go}")
    });
    let body_pos = go.find("\t\tfmt.Println(v)\n").unwrap_or_else(|| {
        panic!("expected the arm body inline in the same block, got:\n{go}")
    });
    assert!(
        inject_pos < body_pos && !go.contains("\t\t{\n"),
        "expected no nested wrap on a non-redefining arm, got:\n{go}"
    );
}

#[test]
fn redefining_unguarded_arm_builds_and_runs_when_go_is_available() {
    let go = emit(
        "fn main() {\n    o := some(2)\n    match o {\n        some(v): { v := v + 1\n            fmt.println(v) }\n        none: fmt.println(0)\n    }\n}\n",
    );
    if std::process::Command::new("go")
        .arg("version")
        .output()
        .is_ok_and(|o| o.status.success())
    {
        let tmp = std::env::temp_dir().join(format!("hy_match_arm_redeclare_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).expect("create temp dir");
        std::fs::write(tmp.join("go.mod"), "module matcharmredeclare\n\ngo 1.21\n")
            .expect("write go.mod");
        std::fs::write(tmp.join("main.go"), &go).expect("write main.go");
        let out = std::process::Command::new("go")
            .arg("run")
            .arg(".")
            .current_dir(&tmp)
            .output()
            .expect("invoke go run");
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(
            out.status.success(),
            "go run failed on emitted Go:\n{}\nstderr:\n{}",
            go,
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("3"),
            "expected the arm-local shadow (2 + 1) to print 3, got:\n{stdout}"
        );
    }
}
