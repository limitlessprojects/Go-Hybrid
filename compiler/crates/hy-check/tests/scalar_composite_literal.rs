//! Regression test: composite literals need a composite type.
//!
//! `check_struct_literal` returned early whenever the literal's type did not
//! resolve to a struct, so `int{}`, `string{}`, `f64{a: 1}`, `bool{}`, `any{}`
//! and `num{}` (with `type num = int`) passed `hyb check` with 0 diagnostics
//! and failed `go build` with `invalid composite literal type ...`.
//!
//! Rule: `BUILDABLE` (GUARANTEES.md Row 13). Diagnostic: `HYB-E1002`.

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("parses");
    check_file(&ast)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn assert_e1002(src: &str) {
    let cs = codes(src);
    assert!(
        cs.contains(&"HYB-E1002".to_string()),
        "expected HYB-E1002 in {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(
        cs.is_empty(),
        "expected 0 diagnostics, got {cs:?} for:\n{src}"
    );
}

#[test]
fn rejects_composite_literal_of_scalar_or_interface_type() {
    for ty in ["int", "string", "bool", "f64", "u8", "any"] {
        assert_e1002(&format!(
            "fn main() {{\n    x := {ty}{{}}\n    fmt.println(x)\n}}\n"
        ));
    }
    // keyed fields do not make a scalar composite
    assert_e1002("fn main() {\n    x := f64{a: 1}\n    fmt.println(x)\n}\n");
    // an alias of a scalar is still a scalar
    assert_e1002("type num = int\nfn main() {\n    x := num{}\n    fmt.println(x)\n}\n");
}

#[test]
fn accepts_struct_literals_including_shadowing_and_aliases() {
    // a user struct named after a predeclared type is a struct
    assert_ok(
        "struct int {\n    v u8\n}\nstruct point {\n    x f64\n}\ntype pt = point\nfn main() {\n    a := int{v: 1}\n    b := pt{x: 1.5}\n    c := point{}\n    fmt.println(a, b, c)\n}\n",
    );
}
