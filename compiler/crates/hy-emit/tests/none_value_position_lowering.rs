//! none_value_position_lowering.rs — emission pins for issue #1297.
//!
//! `none` is the prelude `option[v]` struct's ZERO VALUE (a false `ok`
//! flag), which is why `return none` has always lowered against the
//! declared `option[T]` result. Every OTHER value position used to die in
//! the emitter with "`none` outside a return" while `hyb check` accepted
//! the program — an accepted-but-unbuildable shape under GUARANTEES Row 13
//! (BUILDABLE).
//!
//! The declared type is the evidence the lowering needs, and there are
//! exactly three positions where the emitter has it:
//!   1. a typed `var` declaration initializer (`var o option[int] = none`),
//!   2. a keyed struct-literal field (`wrap{inner: none}`),
//!   3. an assignment to a binding declared `option[T]` (`o = none`).
//! Without a declared target type nothing is lowered: `x := none` has no
//! evidence, stays a clean refusal here, and the checker rejects it first
//! (HYB-E1002, "declared and never used" family).

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit_src(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parses");
    emit_go(&ast).expect("emits")
}

fn emit_err(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parses");
    match emit_go(&ast) {
        Ok(go) => panic!("expected a clean emit refusal, got:\n{go}"),
        Err(e) => format!("{e:?}"),
    }
}

#[test]
fn none_in_a_typed_var_declaration_lowers_to_the_option_zero_value() {
    let go = emit_src(
        "fn main() {\n    var o option[int] = none\n    fmt.println(o.is_none())\n}\n",
    );
    assert!(
        go.contains("var o option[int] = option[int]{}"),
        "the declared option type is the zero value's spelling:\n{go}"
    );
    // The prelude must still be emitted: `option[...]` appears in the body.
    assert!(go.contains("type option[v any] struct"), ":\n{go}");
}

#[test]
fn none_in_a_keyed_struct_literal_field_lowers_from_the_field_type() {
    let go = emit_src(
        "struct wrap {\n    inner option[int]\n}\n\nfn main() {\n    w := wrap{inner: none}\n    fmt.println(w.inner.is_none())\n}\n",
    );
    assert!(go.contains("inner: option[int]{}"), ":\n{go}");
}

#[test]
fn none_assigned_to_a_declared_option_binding_lowers_from_the_binding() {
    let go = emit_src(
        "fn main() {\n    var o option[int] = some(1)\n    o = none\n    fmt.println(o.is_none())\n}\n",
    );
    assert!(go.contains("o = option[int]{}"), ":\n{go}");
    // ... and the same for a binding declared WITHOUT an initializer.
    let go2 = emit_src(
        "fn main() {\n    var o option[int]\n    o = none\n    fmt.println(o.is_none())\n}\n",
    );
    assert!(go2.contains("o = option[int]{}"), ":\n{go2}");
}

#[test]
fn none_as_a_call_argument_stays_a_clean_refusal() {
    // Out of scope on purpose: a call's parameter types are not an expected
    // type the emitter resolves here, so the refusal must survive — never a
    // silently wrong emission.
    let err = emit_err(
        "fn take(o option[int]) {\n    fmt.println(o.is_none())\n}\n\nfn main() {\n    take(none)\n}\n",
    );
    assert!(err.contains("none` outside a return"), "{err}");
}

#[test]
fn a_none_initializer_for_a_non_option_type_is_not_lowered() {
    // `var n int = none` is a type error the CHECKER owns (HYB-E1002); the
    // emitter must not invent a zero value for it.
    let err = emit_err("fn main() {\n    var n int = none\n    fmt.println(n)\n}\n");
    assert!(err.contains("none` outside a return"), "{err}");
}
