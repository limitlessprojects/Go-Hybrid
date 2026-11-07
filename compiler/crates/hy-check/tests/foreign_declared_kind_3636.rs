//! #3636: a declared curated foreign named type (`os.file`, `bufio.reader`,
//! `serde_json.value`, ...) must refuse builtin-scalar interop in every
//! operand position:
//! initializer, assignment, string/return/argument slots, slice-literal
//! elements. Before the fix `kind_of_named` dropped these names to `Unknown`
//! and the `(Unknown, _)` fail-open arms let every shape through with 0
//! diagnostics; `go build` then failed with `cannot use f (variable of struct
//! type os.File) as int value ...`.
//!
//! The deliberately-unclaimed surface is asserted as controls: cross-foreign
//! pairings stay fail-open (interface satisfaction is not modelled here), and
//! `any`/same-name assignments keep passing. Inferred bindings from foreign
//! package CALLS (`f := os.stdin()`) belong to #2997; member naming on
//! foreign values belongs to #2203.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("t.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn rejects(name: &str, src: &str) {
    let r = check(src);
    assert!(
        r.verdict == Verdict::Reject
            && r.diagnostics
                .iter()
                .any(|d| d.severity == Severity::Error && d.code == "HYB-E1002"),
        "{}: expected HYB-E1002 reject, got verdict={:?} {:?}",
        name,
        r.verdict,
        r.diagnostics
    );
}

fn accepts(name: &str, src: &str) {
    let r = check(src);
    assert!(
        r.verdict == Verdict::Accept,
        "{}: expected accept (fail-open surface), got verdict={:?} {:?}",
        name,
        r.verdict,
        r.diagnostics
    );
}

#[test]
fn declared_foreign_rejects_scalar_interop_3636() {
    rejects(
        "var int = foreign",
        "import \"os\"\nfn main() {\n    var f os.file\n    var n int = f\n    fmt.println(n)\n}\n",
    );
    rejects(
        "foreign = int literal",
        "import \"bufio\"\nfn main() {\n    var r bufio.reader\n    r = 1\n    _ = r\n}\n",
    );
    rejects(
        "var string = foreign",
        "import \"os\"\nfn main() {\n    var f os.file\n    var s string = f\n    fmt.println(s)\n}\n",
    );
    rejects(
        "take(int) with foreign arg",
        "import \"os\"\nfn take(n int) { fmt.println(n) }\nfn main() {\n    var f os.file\n    take(f)\n}\n",
    );
    rejects(
        "return foreign as int",
        "import \"os\"\nfn get() int {\n    var f os.file\n    return f\n}\nfn main() { fmt.println(get()) }\n",
    );
    rejects(
        "[]int literal with foreign elem",
        "import \"os\"\nfn main() {\n    var f os.file\n    xs := []int{f}\n    fmt.println(xs)\n}\n",
    );
    // Own-handle types are the same bug class: `serde_json.value` lowers to
    // the `jsonbinding.Value` struct, so `var n int = v` fails `go build`
    // with `cannot use v (variable of struct type jsonbinding.Value) as int
    // value in variable declaration` while `hyb check` stayed silent.
    rejects(
        "var int = foreign own-handle",
        "import \"rust/serde_json\"\nfn main() {\n    var v serde_json.value\n    var n int = v\n    fmt.println(n)\n}\n",
    );
    rejects(
        "var string = foreign own-handle",
        "import \"rust/serde_json\"\nfn main() {\n    var v serde_json.value\n    var s string = v\n    fmt.println(s)\n}\n",
    );
    rejects(
        "take(int) with own-handle arg",
        "import \"rust/serde_json\"\nfn take(n int) int { return n }\nfn main() {\n    var v serde_json.value\n    n := take(v)\n    fmt.println(n)\n}\n",
    );
}

#[test]
fn declared_foreign_fail_open_surface_stays_accepted_3636() {
    accepts(
        "foreign -> any",
        "import \"os\"\nfn main() {\n    var f os.file\n    var a any = f\n    fmt.println(a)\n}\n",
    );
    accepts(
        "foreign -> same foreign",
        "import \"os\"\nfn main() {\n    var f os.file\n    var g os.file = f\n    _ = g\n}\n",
    );
    accepts(
        "own-handle -> any",
        "import \"rust/serde_json\"\nfn main() {\n    var v serde_json.value\n    var a any = v\n    fmt.println(a)\n}\n",
    );
    accepts(
        "own-handle -> same own-handle",
        "import \"rust/serde_json\"\nfn main() {\n    var v serde_json.value\n    var v2 serde_json.value = v\n    _ = v2\n}\n",
    );
    accepts(
        "closure -> foreign interface name",
        "import \"net/http\"\nfn main() {\n    mk := fn() http.handler {\n        return fn(w, r) {\n            _ = w\n            _ = r\n        }\n    }\n    _ = mk\n}\n",
    );
}
