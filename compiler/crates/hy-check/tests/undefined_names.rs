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
fn dotted_first_segment_without_declaration_or_package_rejects_e1001() {
    // Issue #2271: unparenthesized field bases that are not declared locals,
    // parameters, file-level names, or packages on the curated allowlist / imports
    // must reject with HYB-E1001 instead of surviving to go build `undefined: X`.
    rejects_with_e1001("fn main() {\n    x := foo.bar(1)\n    _ = x\n}\n");
    rejects_with_e1001("fn main() {\n    _ = undefined_obj.field\n}\n");
}

#[test]
fn m4_m5_foreign_go_name_and_curated_allowlist_resolution_issue_658() {
    // Known curated package members and initialism constants pass hyb check:
    accepts_without_e1001(
        "fn main() {\n    _ = io.eof\n    _ = http.status_ok\n    _ = time.rfc3339\n    _ = strings.to_valid_utf8(\"a\", \"b\")\n}\n",
    );

    // Non-existent member of curated package is rejected with HYB-E1001:
    rejects_with_e1001("fn main() {\n    fmt.no_such_function(1)\n}\n");

    // Uncurated Go stdlib root is rejected with HYB-E1001:
    rejects_with_e1001("fn main() {\n    _ = math.sqrt(2.0)\n}\n");

    // Explicit import outside curated allowlist is rejected with HYB-E1001 naming the allowlist:
    let r_math = check("import \"math\"\nfn main() {\n    _ = math.sqrt(2.0)\n}\n");
    assert_eq!(r_math.verdict, Verdict::Reject);
    assert!(
        r_math.diagnostics.iter().any(|d| d.code == "HYB-E1001" && d.message.contains("curated allowlist")),
        "expected HYB-E1001 naming curated allowlist for import \"math\", got: {:?}",
        r_math.diagnostics
    );

    let r_third_party = check("import \"github.com/acme/widgets\"\nfn main() {\n    _ = widgets.count()\n}\n");
    assert_eq!(r_third_party.verdict, Verdict::Reject);
    assert!(
        r_third_party.diagnostics.iter().any(|d| d.code == "HYB-E1001" && d.message.contains("curated allowlist")),
        "expected HYB-E1001 naming curated allowlist for third-party import, got: {:?}",
        r_third_party.diagnostics
    );
}

#[test]
fn self_referential_short_binding_rejects_e1001_issue_2271() {
    // Case 2 from issue #2271: `ch := ch` must not resolve RHS against LHS binding
    rejects_with_e1001("fn main() {\n    ch := ch\n    fmt.println(\"{ch}\")\n}\n");
    rejects_with_e1001("fn main() {\n    x := x + 1\n    _ = x\n}\n");
}

#[test]
fn undeclared_field_base_rejects_e1001_issue_2271() {
    // Case 1 from issue #2271: `q.a` must reject with HYB-E1001 when base `q` is undeclared
    rejects_with_e1001("fn main() {\n    fmt.println(\"{q.a}\")\n}\n");
    rejects_with_e1001("fn main() {\n    _ = q.a\n}\n");
    rejects_with_e1001("fn main() {\n    _ = (q).a\n}\n");
}

#[test]
fn positive_controls_issue_2271() {
    // Ordinary := still fine
    accepts_without_e1001("fn main() {\n    ch := make(chan int, 1)\n    fmt.println(\"{ch}\")\n}\n");
    // Declared base, field access
    accepts_without_e1001("struct s { a int }\nfn main() {\n    q := s{a: 1}\n    fmt.println(\"{q.a}\")\n}\n");
}
