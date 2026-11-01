//! Select receive-arm bodies that re-declare a case-bound name at their top
//! level must emit a wrapping block.
//!
//! Go puts a case clause's bindings and its body statements in ONE lexical
//! scope: `case v := <-ch:` followed by `v := v + 1` compiles as `no new
//! variables on left side of :=`. `hyb check` accepts the Hybrid program
//! (arm scoping gives the arm body a fresh scope), so the emitter has to
//! open an explicit block — legal Go, verified against go1.27.2 — and
//! preserves the arm-scoped shadow semantics. Control surfaces assert the
//! no-wrap path stays byte-identical to the previous lowering.

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let file = parse_file("test.hy", src).expect("parse succeeds");
    emit_go(&file).expect("emit succeeds")
}

#[test]
fn wraps_recv_arm_body_redeclaring_the_case_binding() {
    let go = emit(
        r#"package main

import "fmt"

fn main() {
    ch := make(chan int, 1)
    ch <- 5
    select {
        v := <-ch: {
            v := v + 1
            fmt.println(v)
        }
        default: fmt.println(0)
    }
}
"#,
    );
    // The redefining arm body is wrapped in an explicit Go block.
    assert!(
        go.contains("case v := <-ch:\n\t\t{\n"),
        "expected an explicit block after `case v := <-ch:` (Go's clause shares\n\
          one scope with the binding), got:\n{go}"
    );
    // The shadowed re-declaration survives inside the block.
    assert!(
        go.contains("v := v + 1"),
        "expected the arm body's redeclaration to survive, got:\n{go}"
    );
}

#[test]
fn wraps_two_name_recv_arm_redeclaring_only_one_slot() {
    let go = emit(
        r#"package main

import "fmt"

fn main() {
    ch := make(chan int, 1)
    ch <- 5
    select {
        v, ok := <-ch: {
            v := v + 1
            fmt.println(v)
            fmt.println(ok)
        }
        default: fmt.println(0)
    }
}
"#,
    );
    assert!(
        go.contains("case v, ok := <-ch:\n\t\t{\n"),
        "expected `case v, ok := <-ch:` followed by a wrapping block, got:\n{go}"
    );
}

#[test]
fn does_not_wrap_recv_arm_without_top_level_redeclaration() {
    // Byte-exact control: no top-level redeclaration, so the arm body emits
    // flat — exactly the pre-fix lowering (no wrapping braces).
    let go = emit(
        r#"package main

import "fmt"

fn main() {
    ch := make(chan int, 1)
    ch <- 5
    select {
        v := <-ch: fmt.println(v + 1)
        default: fmt.println(0)
    }
}
"#,
    );
    assert!(
        go.contains("case v := <-ch:\n\t\t\tfmt.Println(v + 1)"),
        "expected the flat, pre-identical lowering, got:\n{go}"
    );
    assert!(
        !go.contains("case v := <-ch:\n\t\t{\n"),
        "must not wrap a body that does not re-declare the case binding:\n{go}"
    );
}

#[test]
fn does_not_wrap_recv_arm_shadows_inside_nested_blocks_only() {
    // A re-declaration inside a nested block is a fresh Go scope already:
    // the emitter leaves the flat form untouched.
    let go = emit(
        r#"package main

import "fmt"

fn main() {
    ch := make(chan int, 1)
    ch <- 5
    select {
        v := <-ch: {
            {
                v := v + 1
                fmt.println(v)
            }
        }
        default: fmt.println(0)
    }
}
"#,
    );
    assert!(
        !go.contains("case v := <-ch:\n\t\t{\n\t\t\t{\n"),
        "nested-block re-declaration must not force a redundant extra wrap:\n{go}"
    );
}

#[test]
fn does_not_wrap_default_or_send_arms() {
    let go = emit(
        r#"package main

import "fmt"

fn main() {
    ch := make(chan int, 1)
    cb := make(chan int, 1)
    select {
        cb <- 9: fmt.println(1)
        v := <-ch: fmt.println(v)
        default: {
            v := 7
            _ = v
            fmt.println(0)
        }
    }
}
"#,
    );
    assert!(
        go.contains("default:\n\t\t\tv := 7"),
        "a fresh declaration in a default clause is legal shared-scope Go and\n\
          must stay flat, got:\n{go}"
    );
    assert!(
        go.contains("case cb <- 9:\n\t\t\tfmt.Println(1)"),
        "send arms stay flat, got:\n{go}"
    );
}

#[test]
fn does_not_wrap_read_binding_when_unread_names_blanked_in_head() {
    // One read binding and one unread binding: the unread slot is blanked
    // (`case v, _ :=`) and only the actually-bound name participates in the
    // redeclaration gate (`ok` is never read anywhere, so it blanks).
    let go = emit(
        r#"package main

import "fmt"

fn main() {
    ch := make(chan int, 1)
    ch <- 5
    select {
        v, ok := <-ch: {
            v := v + 1
            fmt.println(v)
            _ = ok
        }
        default: fmt.println(0)
    }
}
"#,
    );
    assert!(
        go.contains("case v, ok := <-ch:\n\t\t{\n"),
        "`ok` is read via `_ = ok`, so both slots are emitted as-is and the\n\
          re-declared `v` forces the wrap, got:\n{go}"
    );
    let go2 = emit(
        r#"package main

import "fmt"

fn main() {
    ch := make(chan int, 1)
    ch <- 5
    select {
        v, oka := <-ch: {
            v := v + 1
            fmt.println(v)
        }
        default: fmt.println(0)
    }
}
"#,
    );
    assert!(
        go2.contains("case v, _ := <-ch:\n\t\t{\n"),
        "the unread slot (`oka`) is blanked; the read slot (`v`) is re-declared\n\
          on the body's top level — wrapped, got:\n{go2}"
    );
}
