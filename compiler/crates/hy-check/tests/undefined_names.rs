//! undefined_names.rs — regression for issue #228 family A: `HYB-E1001`
//! was registered but never constructed, so a value-position name that no
//! declaration provides passed `hyb check` with 0 diagnostics and then
//! failed the emitted Go with `undefined: X` (bare calls, reads, assignment
//! targets, field bases, unknown package roots, `[N]T` array lengths naming
//! a constant that does not exist, file-level const initializers).
//!
//! The allowed set is deliberately over-approximating (every name declared
//! anywhere in the function counts at every use) so scoping shapes the
//! checker already accepts — if-init defines, use-before-define ordering,
//! names defined only inside closures — cannot false-positive. What stays
//! outside the set is exactly the build-breaking class: nothing in the file
//! declares the name at all.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_with_e1001(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1001"),
        "expected HYB-E1001 for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts_without_e1001(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E1001"),
        "must NOT fire HYB-E1001 for {:?}, got {:?}",
        src,
        cs
    );
}

// ---------------------------------------------------------------- reject:

#[test]
fn undefined_call_rejects() {
    rejects_with_e1001("fn main() {\n    undefined_fn()\n}\n");
}

#[test]
fn undefined_read_rejects() {
    rejects_with_e1001("fn main() {\n    y := undefined_var + 1\n    _ = y\n}\n");
}

#[test]
fn undefined_assignment_target_rejects() {
    // Go has no declare-on-assign: `undefined_var = 3` is
    // `undefined: undefined_var` on the emitted program.
    rejects_with_e1001("fn main() {\n    undefined_var = 3\n}\n");
    rejects_with_e1001("fn main() {\n    undefined_obj.field = 3\n}\n");
}

#[test]
fn undefined_array_length_rejects() {
    rejects_with_e1001(
        "struct box {\n    data [undefined_const]int\n}\nfn main() {\n    var b box\n    _ = b\n}\n",
    );
}

#[test]
fn undefined_file_const_value_rejects() {
    rejects_with_e1001("const k = ghost_number\nfn main() {\n    _ = k\n}\n");
}

#[test]
fn undefined_in_closure_body_rejects() {
    // Captures resolve against the enclosing fn's declarations; a name no
    // declaration in the file provides is E1001 inside the closure too.
    rejects_with_e1001("fn main() {\n    f := fn() {\n        _ = ghost\n    }\n    f()\n}\n");
}

#[test]
fn undefined_return_and_defer_rejects() {
    rejects_with_e1001("fn main() {\n    return ghost\n}\n");
    rejects_with_e1001("fn helper() {\n}\nfn main() {\n    defer ghost()\n}\n");
}

#[test]
fn undefined_send_target_rejects() {
    rejects_with_e1001("fn main() {\n    undefined_ch <- 1\n}\n");
}

// ---------------------------------------------------------------- accept:

#[test]
fn if_init_define_visible_to_cond_and_body() {
    accepts_without_e1001("fn main() {\n    if x := 3; x > 0 {\n        _ = x\n    }\n}\n");
}

#[test]
fn builtins_and_conversions_allowed() {
    accepts_without_e1001(
        "fn main() {\n    rows := []string{}\n    rows = append(rows, `hi`)\n    _ = len(rows)\n    v := u8(300)\n    _ = v\n    s := string(65)\n    _ = s\n}\n",
    );
}

#[test]
fn nested_closure_params_and_captures_allowed() {
    accepts_without_e1001(
        "fn main() {\n    f := fn(a int) {\n        g := fn() {\n            _ = a\n        }\n        g()\n    }\n    f(2)\n}\n",
    );
}

#[test]
fn forward_top_level_fn_allowed() {
    accepts_without_e1001("fn main() {\n    helper()\n}\nfn helper() {\n}\n");
}

#[test]
fn named_result_allowed_in_body() {
    accepts_without_e1001("fn main() (code int) {\n    code = 1\n    return\n}\n");
}

#[test]
fn receiver_params_and_fields_allowed() {
    accepts_without_e1001(
        "struct counter {\n    hits int\n}\nfn (c *counter) bump() {\n    c.hits = c.hits + 1\n}\nfn main() {\n    var c counter\n    c.bump()\n}\n",
    );
}

#[test]
fn builtin_package_prefix_allowed() {
    accepts_without_e1001("fn main() {\n    fmt.println(\"hi\")\n}\n");
}

#[test]
fn wildcard_target_allowed() {
    accepts_without_e1001("fn main() {\n    _ = 1\n}\n");
}

#[test]
fn option_surface_allowed() {
    accepts_without_e1001(
        "fn wrap(n int) option[int] {\n    return some(n)\n}\nfn main() {\n    v := wrap(2)\n    match v {\n        some(x): {\n            _ = x\n        }\n        none: {\n        }\n    }\n}\n",
    );
}

#[test]
fn enum_variant_value_allowed() {
    accepts_without_e1001("enum shape {\n    circle\n    square\n}\nfn main() {\n    _ = circle\n}\n");
}

#[test]
fn local_loop_and_select_bindings_allowed() {
    accepts_without_e1001(
        "fn main() {\n    xs := []int{1, 2}\n    total := 0\n    for v in xs {\n        total = total + v\n    }\n    _ = total\n    ch := make(chan int, 1)\n    select {\n        got := <-ch:\n            _ = got\n    }\n}\n",
    );
}

#[test]
fn struct_and_alias_names_allowed_as_values() {
    accepts_without_e1001(
        "type node_id = int\nstruct pair {\n    a int\n    b int\n}\nfn main() {\n    p := pair{a: 1, b: 2}\n    _ = p\n    n := node_id(3)\n    _ = n\n}\n",
    );
}

#[test]
fn imported_package_prefix_allowed() {
    accepts_without_e1001(
        "import \"rust/serde_json\"\nfn main() {\n    doc := serde_json.parse(\"[]\")?\n    _ = doc\n}\n",
    );
}

#[test]
fn dotted_first_segment_is_package_open_world() {
    // The emitter's contract: "unknown roots still work — they emit verbatim
    // and get an import" (hy-emit is_package_root). Spike-10's `hex.encode`
    // is a root the checker's foreign table never listed but that builds, so
    // a direct ident base under `.` is exempt from E1001 on purpose — the
    // checker cannot tell a real std/binding root from a typo'd one, and the
    // go toolchain reports the difference exactly (`undefined: foo` vs a
    // working import). Everything BARE stays checked.
    accepts_without_e1001("fn main() {\n    x := foo.bar(1)\n    _ = x\n}\n");
    accepts_without_e1001("fn main() {\n    _ = undefined_obj.field\n}\n");
}
