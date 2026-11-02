//! issue_3120_generic_struct_literal_field_widths.rs — issue #3120 (Row 13 BUILDABLE).
//!
//! A typed integer value written into a field of a generic struct literal
//! (`box[int8]{v: z}` with `z int64`) escaped the #2712 scalar width check
//! because field type parameters were not substituted before looking up the field scalar.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn generic_struct_literal_field_scalar_mismatch_rejects_e1002() {
    let cases = [
        (
            "short define: b := box[int8]{v: z}",
            "struct box[t any] { v t }\nfn main() {\n    var z int64 = 1\n    b := box[int8]{v: z}\n    _ = b\n}\n",
        ),
        (
            "typed var: var b box[int8] = box[int8]{v: z}",
            "struct box[t any] { v t }\nfn main() {\n    var z int64 = 1\n    var b box[int8] = box[int8]{v: z}\n    _ = b\n}\n",
        ),
        (
            "slice literal item: bs := []box[int8]{box[int8]{v: z}}",
            "struct box[t any] { v t }\nfn main() {\n    var z int64 = 1\n    bs := []box[int8]{box[int8]{v: z}}\n    _ = bs\n}\n",
        ),
        (
            "multiple type params: pair[string, int8]{a: \"x\", b: z}",
            "struct pair[k any, v any] { a k\n    b v }\nfn main() {\n    var z int64 = 1\n    p := pair[string, int8]{a: \"x\", b: z}\n    _ = p\n}\n",
        ),
        (
            "non-generic control: pt{x: z} with x int8 and z int64",
            "struct pt { x int8 }\nfn main() {\n    var z int64 = 1\n    p := pt{x: z}\n    _ = p\n}\n",
        ),
    ];

    for (name, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "expected reject for case: {name}\nsrc:\n{src}"
        );
        let codes = error_codes(&r);
        assert!(
            codes.contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 in {codes:?} for case: {name}"
        );
    }
}

#[test]
fn generic_struct_literal_valid_controls_accept() {
    let cases = [
        (
            "matching typed scalar int8",
            "struct box[t any] { v t }\nfn main() {\n    var z8 int8 = 1\n    b := box[int8]{v: z8}\n    _ = b\n}\n",
        ),
        (
            "untyped int literal to int64 generic",
            "struct box[t any] { v t }\nfn main() {\n    b := box[int64]{v: 2}\n    _ = b\n}\n",
        ),
        (
            "matching typed scalar in multi-param generic struct",
            "struct pair[k any, v any] { a k\n    b v }\nfn main() {\n    var z8 int8 = 1\n    p := pair[string, int8]{a: \"x\", b: z8}\n    _ = p\n}\n",
        ),
        (
            "matching typed scalar in slice literal",
            "struct box[t any] { v t }\nfn main() {\n    var z8 int8 = 1\n    bs := []box[int8]{box[int8]{v: z8}}\n    _ = bs\n}\n",
        ),
    ];

    for (name, src) in cases {
        let r = check(src);
        assert_eq!(
            r.verdict,
            Verdict::Accept,
            "expected accept for valid case: {name}, got {r:?}\nsrc:\n{src}"
        );
    }
}
