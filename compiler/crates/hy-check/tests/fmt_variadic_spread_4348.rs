//! Regression tests for #4348 — `...` spread arguments on the curated
//! `fmt` print/scan/append family.
//!
//! Go accepts a spread on a variadic call only when it is the final
//! argument, only after exactly the fixed leading parameters, and only
//! when the operand is assignable to the variadic slice (`[]any` for the
//! `fmt` family, whose slices are invariant). Before this, every shape
//! below passed `hyb check` with 0 diagnostics and then failed `go build`
//! (Row 13 BUILDABLE: reject fail-closed, HYB-E1002).
//!
//! Acceptance controls pin the other half: `[]any`, sub-slices, `[]any`
//! aliases (including an alias of `any` used as the element type), calls
//! returning `[]any`, and `nil...` (assignable to `[]any`) must stay clean.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
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

fn rejects_e1002(src: &str) {
    let r = check(src);
    let codes = error_codes(&r);
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for:\n{src}\ngot: {codes:?}\n{:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{src}");
}

fn accepts(src: &str) {
    let r = check(src);
    let codes = error_codes(&r);
    assert!(
        codes.is_empty(),
        "expected 0 errors for:\n{src}\ngot: {codes:?}\n{:?}",
        r.diagnostics
    );
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept for:\n{src}");
}

// ---------------------------------------------------------------------------
// The operand is not `[]any`
// ---------------------------------------------------------------------------

#[test]
fn non_any_slices_and_scalars_reject() {
    for src in [
        // []string is not assignable to []any
        "import \"fmt\"\nfn main() {\n    names := []string{\"a\", \"b\"}\n    fmt.println(names...)\n}\n",
        // []int
        "import \"fmt\"\nfn main() {\n    xs := []int{1, 2}\n    fmt.printf(\"%d %d\\n\", xs...)\n}\n",
        // []error
        "import \"fmt\"\nfn main() {\n    xs := []error{}\n    fmt.println(xs...)\n}\n",
        // []byte — slice element types are invariant, so it is not []any
        "import \"fmt\"\nfn main() {\n    bs := []byte{65}\n    fmt.print(bs...)\n}\n",
        // scalar
        "import \"fmt\"\nfn main() {\n    n := 3\n    fmt.println(n...)\n}\n",
    ] {
        rejects_e1002(src);
    }
}

#[test]
fn type_parameter_slice_rejects() {
    // Go refuses `[]T` as `[]any` even when T is constrained to any; Hybrid
    // generics lower to Go generics, so this must reject fail-closed.
    rejects_e1002(
        "import \"fmt\"\nfn show[t](xs []t) {\n    fmt.println(xs...)\n}\nfn main() {\n    show[int]([]int{1})\n}\n",
    );
}

// ---------------------------------------------------------------------------
// The spread is not the final argument, or is repeated
// ---------------------------------------------------------------------------

#[test]
fn non_final_or_repeated_spread_rejects() {
    for src in [
        "import \"fmt\"\nfn main() {\n    xs := []any{1}\n    e := fmt.errorf(\"x %d\", xs..., 2)\n    fmt.println(e)\n}\n",
        "import \"fmt\"\nfn main() {\n    xs := []any{1}\n    s := fmt.sprint(xs..., xs...)\n    fmt.println(s)\n}\n",
    ] {
        rejects_e1002(src);
    }
}

// ---------------------------------------------------------------------------
// The leading arguments do not equal the member's fixed parameters
// ---------------------------------------------------------------------------

#[test]
fn mixed_individual_values_and_spread_reject() {
    for src in [
        // Println has 0 fixed parameters.
        "import \"fmt\"\nfn main() {\n    xs := []any{1}\n    fmt.println(\"x\", xs...)\n}\n",
        // Printf has 1 fixed parameter (format).
        "import \"fmt\"\nfn main() {\n    xs := []any{1}\n    fmt.printf(\"%v\", 1, xs...)\n}\n",
        // Fprintf has 2 fixed parameters (writer, format).
        "import \"fmt\"\nimport \"os\"\nfn main() {\n    xs := []any{1}\n    fmt.fprintf(os.stdout, \"%v\", 1, xs...)\n}\n",
        // Appendf has 2 fixed parameters (dst, format).
        "import \"fmt\"\nfn main() {\n    xs := []any{1}\n    fmt.appendf([]byte{}, \"%v\", 1, xs...)\n}\n",
    ] {
        rejects_e1002(src);
    }
}

#[test]
fn missing_fixed_parameters_before_spread_reject() {
    for src in [
        // Fprintln needs the writer.
        "import \"fmt\"\nfn main() {\n    xs := []any{1}\n    fmt.fprintln(xs...)\n}\n",
        // Printf needs the format.
        "import \"fmt\"\nfn main() {\n    xs := []any{1}\n    fmt.printf(xs...)\n}\n",
        // Eprintf lowers to Fprintf(os.Stderr, format, ...): still needs format.
        "import \"fmt\"\nfn main() {\n    xs := []any{1}\n    fmt.eprintf(xs...)\n}\n",
        // Fprintf needs writer and format.
        "import \"fmt\"\nimport \"os\"\nfn main() {\n    xs := []any{1}\n    fmt.fprintf(os.stdout, xs...)\n}\n",
        // Append needs the destination []byte.
        "import \"fmt\"\nfn main() {\n    xs := []any{1}\n    fmt.append(xs...)\n}\n",
        // Sscan needs the input string; Sscanf needs string and format.
        "import \"fmt\"\nfn main() {\n    xs := []any{1}\n    fmt.sscan(xs...)\n}\n",
        "import \"fmt\"\nfn main() {\n    xs := []any{1}\n    fmt.sscanf(xs...)\n}\n",
        // Fscan needs the reader; Fscanf needs reader and format.
        "import \"fmt\"\nfn main() {\n    xs := []any{1}\n    fmt.fscan(xs...)\n}\n",
        "import \"fmt\"\nfn main() {\n    xs := []any{1}\n    fmt.fscanf(xs...)\n}\n",
    ] {
        rejects_e1002(src);
    }
}

// ---------------------------------------------------------------------------
// Valid spreads stay accepted
// ---------------------------------------------------------------------------

#[test]
fn any_slice_spreads_accept() {
    accepts(
        r#"import "fmt"
import "os"

type any_alias = any
type list = []any

fn vals() []any {
    return []any{1, "b"}
}

fn main() {
    xs := []any{1, "two"}
    fmt.println(xs...)
    fmt.print(xs[1:]...)
    fmt.printf("%v %v\n", xs...)
    s := fmt.sprintf("%v-%v", xs...)
    fmt.println(s)
    s2 := fmt.sprint(xs...)
    fmt.println(s2)
    s3 := fmt.sprintln(xs...)
    fmt.println(s3)
    e := fmt.errorf("e %v", xs...)
    fmt.println(e)
    fmt.fprintln(os.stdout, xs...)
    fmt.fprintf(os.stdout, "%v %v\n", xs...)
    fmt.eprintln(xs...)
    fmt.eprintf("%v\n", xs...)
    fmt.println(vals()...)
    var l list = []any{1, 2}
    fmt.println(l...)
    ys := []any_alias{1, "three"}
    fmt.println(ys...)
}
"#,
    );
}

#[test]
fn append_family_any_spreads_accept() {
    accepts(
        r#"import "fmt"

fn main() {
    xs := []any{1, "two"}
    dst := []byte{}
    dst = fmt.append(dst, xs...)
    dst = fmt.appendf(dst, "%v", xs...)
    dst = fmt.appendln(dst, xs...)
    fmt.println(len(dst))
}
"#,
    );
}

#[test]
fn scan_family_any_spreads_accept() {
    accepts(
        r#"import "fmt"
import "os"

fn main() {
    xs := []any{1}
    fmt.scan(xs...)
    fmt.scanln(xs...)
    fmt.sscan("1", xs...)
    fmt.sscanln("1", xs...)
    fmt.sscanf("1 2", "%d %d", xs...)
    fmt.fscan(os.stdin, xs...)
    fmt.fscanln(os.stdin, xs...)
    fmt.fscanf(os.stdin, "%d", xs...)
}
"#,
    );
}

#[test]
fn nil_spread_accepts() {
    // nil is assignable to []any; Go builds `fmt.Println(nil...)`.
    accepts("import \"fmt\"\nfn main() {\n    fmt.println(nil...)\n}\n");
}

#[test]
fn calls_without_spread_are_unaffected() {
    accepts(
        r#"import "fmt"
import "os"

fn main() {
    xs := []string{"a", "b"}
    fmt.println(xs)
    fmt.println("a", 1, xs)
    fmt.printf("%v %d\n", xs, 2)
    fmt.fprintln(os.stdout, xs, 2)
}
"#,
    );
}
