//! field_method_name_clash.rs — a struct field and a method with the same
//! name. Go rejects the emitted type: `field and method with the same name n`.
//! `hyb check` must reject it fail-closed as HYB-E1002 (rule `BUILDABLE`,
//! GUARANTEES.md Row 13, fixes #1119).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|d| panic!("must parse (parser contract): {:?}", d));
    check_file(&ast)
}

fn assert_rejects_e1002(src: &str, what: &str) {
    let r = check(src);
    let codes: Vec<String> = r
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect();
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {}, got {:?}",
        what,
        codes
    );
    assert_eq!(r.verdict, Verdict::Reject, "{}", what);
}

const VALUE_RECEIVER: &str = "\
struct counter {
    n int
}

fn (c counter) n() int {
    return 1
}

fn main() {
    c := counter{n: 2}
    fmt.println(\"{c.n}\")
}
";

const POINTER_RECEIVER: &str = "\
struct counter {
    n int
}

fn (c *counter) n() int {
    return 1
}

fn main() {
    c := counter{n: 2}
    fmt.println(\"{c.n}\")
}
";

const FORWARD_STRUCT_DECL: &str = "\
fn (c counter) n() int {
    return 1
}

struct counter {
    n int
}

fn main() {
    c := counter{n: 2}
    fmt.println(\"{c.n}\")
}
";

const GENERIC_STRUCT_RECEIVER: &str = "\
struct box[t] {
    val t
}

fn (b box[t]) val() t {
    return b.val
}

fn main() {
    b := box[int]{val: 5}
    fmt.println(\"{b.val}\")
}
";

const DISTINCT_NAMES_OK: &str = "\
struct counter {
    n int
}

fn (c counter) get() int {
    return c.n
}

fn main() {
    c := counter{n: 2}
    fmt.println(\"{c.get()}\")
}
";

#[test]
fn field_and_value_method_same_name_rejects() {
    assert_rejects_e1002(VALUE_RECEIVER, "field `n` + value method `n`");
}

#[test]
fn field_and_pointer_method_same_name_rejects() {
    assert_rejects_e1002(POINTER_RECEIVER, "field `n` + pointer method `n`");
}

#[test]
fn field_and_method_forward_declared_struct_rejects() {
    assert_rejects_e1002(FORWARD_STRUCT_DECL, "forward-declared struct with field/method clash");
}

#[test]
fn field_and_method_generic_struct_rejects() {
    assert_rejects_e1002(GENERIC_STRUCT_RECEIVER, "generic struct field/method clash");
}

#[test]
fn distinct_field_and_method_names_still_accept() {
    let r = check(DISTINCT_NAMES_OK);
    assert_ne!(
        r.verdict,
        Verdict::Reject,
        "control program must stay accepted: {:?}",
        r.diagnostics
    );
}
