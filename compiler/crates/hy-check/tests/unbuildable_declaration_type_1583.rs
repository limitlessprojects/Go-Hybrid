//! unbuildable_declaration_type_1583.rs — regression for issue #1583:
//! [buildable] a declared type is never resolved: `struct s { a missing }`,
//! `var x missing`, `[]missing`, `map[missing]int`, `box[missing]` pass
//! hyb check, go build says `undefined: missing`.
//!
//! Every type position in declarations must resolve against predeclared types,
//! declared file structs, zero-field structs, enums, type aliases, and in-scope
//! type parameters. Unresolved names reject fail-closed as HYB-E1002.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn error_messages(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.message.clone())
        .collect()
}

// ---------------------------------------------------------------------------
// The 13 repro shapes from Issue #1583
// ---------------------------------------------------------------------------

#[test]
fn shape_01_struct_field_undefined_type_rejects_e1002() {
    let r = check("struct s {\n  a missing\n}\nfn main() {\n  var v s\n  fmt.println(v.a)\n}");
    assert_eq!(r.verdict, Verdict::Reject);
    assert_eq!(error_codes(&r), vec!["HYB-E1002"]);
    assert!(error_messages(&r)[0].contains("undefined: missing"));
}

#[test]
fn shape_02_local_var_undefined_type_rejects_e1002() {
    let r = check("fn main() {\n  var x missing\n  fmt.println(x)\n}");
    assert_eq!(r.verdict, Verdict::Reject);
    assert_eq!(error_codes(&r), vec!["HYB-E1002"]);
    assert!(error_messages(&r)[0].contains("undefined: missing"));
}

#[test]
fn shape_03_fn_param_undefined_type_rejects_e1002() {
    let r = check("fn f(p missing) int {\n  return 1\n}\nfn main() {\n  fmt.println(f(1))\n}");
    assert_eq!(r.verdict, Verdict::Reject);
    assert_eq!(error_codes(&r), vec!["HYB-E1002"]);
    assert!(error_messages(&r)[0].contains("undefined: missing"));
}

#[test]
fn shape_04_fn_result_undefined_type_rejects_e1002() {
    let r = check("fn f() missing {\n  panic(\"no\")\n}\nfn main() {\n  f()\n}");
    assert_eq!(r.verdict, Verdict::Reject);
    assert_eq!(error_codes(&r), vec!["HYB-E1002"]);
    assert!(error_messages(&r)[0].contains("undefined: missing"));
}

#[test]
fn shape_05_type_alias_undefined_target_rejects_e1002() {
    let r = check("type t = missing\nfn main() {\n  var x t\n  fmt.println(x)\n}");
    assert_eq!(r.verdict, Verdict::Reject);
    assert_eq!(error_codes(&r), vec!["HYB-E1002"]);
    assert!(error_messages(&r)[0].contains("undefined: missing"));
}

#[test]
fn shape_06_slice_element_undefined_type_rejects_e1002() {
    let r = check("fn main() {\n  var xs []missing\n  fmt.println(xs)\n}");
    assert_eq!(r.verdict, Verdict::Reject);
    assert_eq!(error_codes(&r), vec!["HYB-E1002"]);
    assert!(error_messages(&r)[0].contains("undefined: missing"));
}

#[test]
fn shape_07_map_key_undefined_type_rejects_e1002() {
    let r = check("fn main() {\n  var m map[missing]int\n  fmt.println(m)\n}");
    assert_eq!(r.verdict, Verdict::Reject);
    assert_eq!(error_codes(&r), vec!["HYB-E1002"]);
    assert!(error_messages(&r)[0].contains("undefined: missing"));
}

#[test]
fn shape_08_map_value_undefined_type_rejects_e1002() {
    let r = check("fn main() {\n  var m map[string]missing\n  fmt.println(m)\n}");
    assert_eq!(r.verdict, Verdict::Reject);
    assert_eq!(error_codes(&r), vec!["HYB-E1002"]);
    assert!(error_messages(&r)[0].contains("undefined: missing"));
}

#[test]
fn shape_09_chan_element_undefined_type_rejects_e1002() {
    let r = check("fn main() {\n  var c chan missing\n  fmt.println(c)\n}");
    assert_eq!(r.verdict, Verdict::Reject);
    assert_eq!(error_codes(&r), vec!["HYB-E1002"]);
    assert!(error_messages(&r)[0].contains("undefined: missing"));
}

#[test]
fn shape_10_option_payload_undefined_type_rejects_e1002() {
    let r = check("fn main() {\n  var o option[missing]\n  fmt.println(o)\n}");
    assert_eq!(r.verdict, Verdict::Reject);
    assert_eq!(error_codes(&r), vec!["HYB-E1002"]);
    assert!(error_messages(&r)[0].contains("undefined: missing"));
}

#[test]
fn shape_11_array_element_undefined_type_rejects_e1002() {
    let r = check("fn main() {\n  var a [2]missing\n  fmt.println(a)\n}");
    assert_eq!(r.verdict, Verdict::Reject);
    assert_eq!(error_codes(&r), vec!["HYB-E1002"]);
    assert!(error_messages(&r)[0].contains("undefined: missing"));
}

#[test]
fn shape_12_generic_type_arg_undefined_rejects_e1002() {
    let r = check("struct box[t] {\n  v t\n}\nfn main() {\n  var b box[missing]\n  fmt.println(b)\n}");
    assert_eq!(r.verdict, Verdict::Reject);
    assert_eq!(error_codes(&r), vec!["HYB-E1002"]);
    assert!(error_messages(&r)[0].contains("undefined: missing"));
}

#[test]
fn shape_13_explicit_const_undefined_type_rejects_e1002() {
    let r = check("const c missing = 1\nfn main() {\n  fmt.println(c)\n}");
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    assert!(error_messages(&r).iter().any(|m| m.contains("undefined: missing")));
}

// ---------------------------------------------------------------------------
// Positive controls that must stay accepted
// ---------------------------------------------------------------------------

#[test]
fn positive_controls_declared_struct_marker_enum_alias_stay_accepted() {
    let r = check(
        "struct s {\n\
             a int\n\
         }\n\
         struct marker {}\n\
         enum color { red, green }\n\
         type alias_t = int\n\
         fn main() {\n\
             var v1 s\n\
             var v2 marker\n\
             var v3 color\n\
             var v4 alias_t\n\
             fmt.println(v1, v2, v3, v4)\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn positive_controls_generic_struct_params_stay_accepted() {
    let r = check(
        "struct box[t] {\n\
             items []t\n\
             table map[string]t\n\
             maybe option[t]\n\
         }\n\
         fn main() {\n\
             var b box[int]\n\
             fmt.println(b)\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn positive_controls_generic_fn_param_and_result_stay_accepted() {
    let r = check(
        "fn id[t any](x t) t {\n\
             return x\n\
         }\n\
         fn main() {\n\
             fmt.println(id[int](42))\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn positive_controls_predeclared_and_spec_types_stay_accepted() {
    let r = check(
        "fn main() {\n\
             var a bool\n\
             var b byte\n\
             var c int\n\
             var d int64\n\
             var e float64\n\
             var f string\n\
             var g error\n\
             var h any\n\
             var i rune\n\
             var j uintptr\n\
             var k u8\n\
             var l i8\n\
             var m f32\n\
             var n f64\n\
             fmt.println(a, b, c, d, e, f, g, h, i, j, k, l, m, n)\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn positive_controls_dotted_foreign_type_stays_accepted() {
    let r = check(
        "import \"rust/serde_json\"\n\
         struct holder {\n\
             v serde_json.value\n\
         }\n\
         fn main() {\n\
             var h holder\n\
             fmt.println(h)\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn positive_controls_prelude_types_option_and_set_stay_accepted() {
    let r = check(
        "fn main() {\n\
             var o option[int]\n\
             var s set[string]\n\
             fmt.println(o, s)\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn positive_controls_type_param_as_generic_arg_stays_accepted() {
    let r = check(
        "struct box[t] {\n\
             inner box[t]\n\
         }\n\
         fn main() {\n\
             fmt.println(\"ok\")\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}
