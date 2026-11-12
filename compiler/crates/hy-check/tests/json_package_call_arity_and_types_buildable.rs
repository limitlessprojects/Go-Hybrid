//! json_package_call_arity_and_types_buildable.rs — regression tests for encoding/json package calls
//! (HYB-E1002 arity and non-callable type rejection, Row 13 BUILDABLE).
//!
//! Go standard library contract:
//! - Package types like `json.Decoder`, `json.Encoder`, `json.RawMessage`, `json.Number`, `json.Delim`,
//!   `json.SyntaxError`, etc. are types or interfaces, NOT functions. Calling them as functions fails
//!   `go build` with `cannot call non-function json.<Name>`.
//! - Functions like `json.Marshal(v)` (1 arg), `json.Unmarshal(data, v)` (2 args), `json.MarshalIndent(v, prefix, indent)` (3 args),
//!   `json.NewDecoder(r)` (1 arg), `json.NewEncoder(w)` (1 arg), `json.Valid(data)` (1 arg), `json.Compact(dst, src)` (2 args),
//!   and `json.HTMLEscape(dst, src)` (2 args) have fixed arity and are non-variadic.
//! - Passing wrong argument counts or calling types as functions produces unbuildable Go code under `go build`.
//! - `hy-check` must reject these calls with `HYB-E1002` under GUARANTEES.md Row 13.

use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
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
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for:\n{}\ngot: {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.is_empty(),
        "expected Accept with 0 errors for:\n{}\ngot: {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept for:\n{}", src);
}

// ---------------------------------------------------------------------------
// Non-callable package types rejected as call targets
// ---------------------------------------------------------------------------

#[test]
fn test_json_non_callable_types_reject() {
    rejects_e1002("import \"json\"\nfn main() {\n    _ = json.decoder()\n}\n");
    rejects_e1002("import \"json\"\nfn main() {\n    _ = json.encoder()\n}\n");
    rejects_e1002("import \"json\"\nfn main() {\n    _ = json.raw_message()\n}\n");
    rejects_e1002("import \"json\"\nfn main() {\n    _ = json.number()\n}\n");
    rejects_e1002("import \"json\"\nfn main() {\n    _ = json.delim()\n}\n");
    rejects_e1002("import \"json\"\nfn main() {\n    _ = json.syntax_error()\n}\n");
    rejects_e1002("import \"json\"\nfn main() {\n    _ = json.marshaler()\n}\n");
    rejects_e1002("import \"json\"\nfn main() {\n    _ = json.unmarshaler()\n}\n");
}

// ---------------------------------------------------------------------------
// Arity violations on callable json functions
// ---------------------------------------------------------------------------

#[test]
fn test_json_marshal_arity_rejects() {
    rejects_e1002("import \"json\"\nfn main() {\n    _ = json.marshal()\n}\n");
    rejects_e1002("import \"json\"\nfn main() {\n    _ = json.marshal(1, 2)\n}\n");
}

#[test]
fn test_json_unmarshal_arity_rejects() {
    rejects_e1002("import \"json\"\nfn main() {\n    _ = json.unmarshal()\n}\n");
    rejects_e1002("import \"json\"\nfn main() {\n    _ = json.unmarshal(1)\n}\n");
    rejects_e1002("import \"json\"\nfn main() {\n    _ = json.unmarshal(1, 2, 3)\n}\n");
}

#[test]
fn test_json_marshal_indent_arity_rejects() {
    rejects_e1002("import \"json\"\nfn main() {\n    _ = json.marshal_indent()\n}\n");
    rejects_e1002("import \"json\"\nfn main() {\n    _ = json.marshal_indent(1)\n}\n");
    rejects_e1002("import \"json\"\nfn main() {\n    _ = json.marshal_indent(1, \"\")\n}\n");
    rejects_e1002("import \"json\"\nfn main() {\n    _ = json.marshal_indent(1, \"\", \"  \", 4)\n}\n");
}

#[test]
fn test_json_new_decoder_encoder_arity_rejects() {
    rejects_e1002("import \"json\"\nfn main() {\n    _ = json.new_decoder()\n}\n");
    rejects_e1002("import \"json\"\nfn main() {\n    _ = json.new_encoder()\n}\n");
    rejects_e1002("import \"json\"\nfn main() {\n    _ = json.new_decoder(1, 2)\n}\n");
}

#[test]
fn test_json_valid_compact_indent_arity_rejects() {
    rejects_e1002("import \"json\"\nfn main() {\n    _ = json.valid()\n}\n");
    rejects_e1002("import \"json\"\nfn main() {\n    _ = json.valid(1, 2)\n}\n");
    rejects_e1002("import \"json\"\nfn main() {\n    _ = json.compact()\n}\n");
    rejects_e1002("import \"json\"\nfn main() {\n    _ = json.compact(1)\n}\n");
    rejects_e1002("import \"json\"\nfn main() {\n    _ = json.indent(1, 2, 3)\n}\n");
}

// ---------------------------------------------------------------------------
// Variadic spread on non-variadic functions
// ---------------------------------------------------------------------------

#[test]
fn test_json_variadic_spread_rejects() {
    rejects_e1002("import \"json\"\nfn main() {\n    items := []int{1, 2}\n    _ = json.marshal(items...)\n}\n");
}

// ---------------------------------------------------------------------------
// Positive controls: valid calls accepted
// ---------------------------------------------------------------------------

#[test]
fn test_json_valid_calls_accept() {
    let src = r#"
import "json"
import "os"

fn main() {
    data, _ := json.marshal(42)
    var target int
    _ = json.unmarshal(data, &target)
    formatted, _ := json.marshal_indent(42, "", "  ")
    _ = json.valid(data)
    _ = json.new_decoder(os.stdin)
    _ = json.new_encoder(os.stdout)
    _ = formatted
}
"#;
    accepts(src);
}

#[test]
fn test_json_shadowed_package_keeps_user_semantics() {
    let src = r#"
struct my_json {
    val int
}

fn (j my_json) decoder() int {
    return j.val
}

fn main() {
    json := my_json{val: 99}
    _ = json.decoder()
}
"#;
    accepts(src);
}
