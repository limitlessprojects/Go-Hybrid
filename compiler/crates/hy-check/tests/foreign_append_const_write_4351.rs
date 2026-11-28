//! Regression (#4351): `fmt.append*` and `strconv.append_*` write in place
//! into spare capacity just like builtin `append`, so bounded reslices of a
//! `const` slice must reject with `HYB-E3006` while full-capacity slices stay legal.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_e3006(src: &str, what: &str) {
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3006"),
        "{}: expected HYB-E3006, got {:?}",
        what,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "{}", what);
}

#[test]
fn package_append_into_const_reslice_rejected_e3006() {
    for (import_pkg, call) in [
        ("fmt", "fmt.append(table[:0], 11)"),
        ("fmt", "fmt.appendf(table[:1], \"%d\", 22)"),
        ("fmt", "fmt.appendln(table[0:2], 33)"),
        ("strconv", "strconv.append_bool(table[:0], true)"),
        ("strconv", "strconv.append_int(table[:1], 42, 10)"),
        ("strconv", "strconv.append_uint(table[:1], 42, 10)"),
        ("strconv", "strconv.append_float(table[:0], 3.14, \"f\", 2, 64)"),
        ("strconv", "strconv.append_quote(table[:0], \"hi\")"),
    ] {
        let src = format!(
            "import \"{import_pkg}\"\n\
             const table = []byte{{48, 48, 48, 48, 48, 48, 48, 48}}\n\
             fn main() {{\n\
                 out := {call}\n\
                 _ = out\n\
             }}\n"
        );
        assert_e3006(&src, call);
    }

    assert_e3006(
        "import \"fmt\"\n\
         const table = []byte{48, 48, 48, 48}\n\
         fn main() {\n\
             view := table[:2]\n\
             _ = fmt.append(view, 99)\n\
         }\n",
        "alias of const reslice passed to fmt.append",
    );
}

#[test]
fn package_append_full_const_or_mutable_slice_accepted() {
    let r = check(
        "import \"fmt\"\n\
         import \"strconv\"\n\
         const table = []byte{48, 48, 48, 48}\n\
         fn main() {\n\
             a := fmt.append(table, 11)\n\
             b := fmt.append(table[1:], 12)\n\
             c := strconv.append_int(table, 42, 10)\n\
             var buf = make([]byte, 0, 16)\n\
             d := fmt.appendf(buf, \"%d\", 7)\n\
             e := strconv.append_quote(d[:0], \"ok\")\n\
             _ = a\n\
             _ = b\n\
             _ = c\n\
             _ = e\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Accept, "got {:?}", r.diagnostics);
}
