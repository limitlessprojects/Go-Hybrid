//! #3029 — a function-local *value* binding whose name is also a type name
//! shadows the type in Go's scope, so a later type use emits unbuildable Go
//! (`X (local variable) is not a type`, GUARANTEES row 13 BUILDABLE).

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn run(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn rejects_e1002(src: &str) -> bool {
    let r = run(src);
    r.verdict == Verdict::Reject && r.diagnostics.iter().any(|d| d.code == "HYB-E1002")
}

#[test]
fn predeclared_type_shadowed_by_local_then_used_is_rejected() {
    // issue #3029 rows 1–3, 6–8
    assert!(rejects_e1002(
        "fn main() {\n    int := 1\n    var y int = 2\n    fmt.println(int, y)\n}"
    ));
    assert!(rejects_e1002(
        "fn main() {\n    string := \"a\"\n    var t string = \"b\"\n}"
    ));
    assert!(rejects_e1002(
        "fn main() {\n    any := 1\n    var x any = 2\n}"
    ));
    assert!(rejects_e1002(
        "fn main() {\n    bool := 1\n    xs := []bool{true}\n}"
    ));
    assert!(rejects_e1002(
        "fn main() {\n    int := 1\n    m := map[string]int{\"a\": 2}\n}"
    ));
    assert!(rejects_e1002(
        "fn main() {\n    byte := 1\n    bs := []byte(\"s\")\n}"
    ));
}

#[test]
fn parameter_shadowing_a_type_then_used_is_rejected() {
    // issue #3029 row 4
    assert!(rejects_e1002(
        "fn f(int int) int {\n    var y int = int\n    return y\n}"
    ));
}

#[test]
fn loop_binding_shadowing_a_type_then_used_is_rejected() {
    // issue #3029 row 5
    assert!(rejects_e1002(
        "fn main() {\n    xs := [1, 2]\n    for string in xs {\n        var t string = \"a\"\n    }\n}"
    ));
}

#[test]
fn struct_name_shadowed_by_local_then_reused_is_rejected() {
    // issue #3029 row 9
    assert!(rejects_e1002(
        "struct p {\n    x int\n}\nfn main() {\n    p := p{x: 1}\n    q := p{x: 2}\n    fmt.println(p, q)\n}"
    ));
}

#[test]
fn shadow_alone_and_out_of_scope_uses_are_accepted() {
    // The shadow without a later type use is fine.
    assert_eq!(
        run("fn main() {\n    int := 1\n    fmt.println(int)\n}").verdict,
        Verdict::Accept
    );
    // A type use in an inner block whose shadowing binding dies at the block.
    assert_eq!(
        run("fn main() {\n    {\n        int := 1\n        fmt.println(int)\n    }\n    var y int = 2\n    fmt.println(y)\n}")
            .verdict,
        Verdict::Accept
    );
    // `p := p{...}` is fine: Go scopes the new name AFTER its own initializer,
    // so the first literal still names the struct type.
    assert_eq!(
        run("struct p {\n    x int\n}\nfn main() {\n    p := p{x: 1}\n    fmt.println(p)\n}").verdict,
        Verdict::Accept
    );
}
