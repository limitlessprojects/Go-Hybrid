//! Regression tests for #4260:
//! The full `encoding/json` call surface must be arity-checked at `hyb check`
//! time. Before this, every member the member table in foreign.rs blesses
//! escaped with arbitrary arity and non-callable package-level types and errors
//! (`decoder`, `encoder`, `raw_message`, `delim`, `number`, `marshaler`,
//! `unmarshaler`, `syntax_error`, `unmarshal_type_error`,
//! `unsupported_type_error`, `unsupported_value_error`, `marshaler_error`)
//! could be called as functions — both blow up at `go build` instead (Row 13
//! BUILDABLE: reject fail-closed, HYB-E1002).
//!
//! Arities mirror the Go std signatures each member lowers to via
//! `to_go_name` (snake -> CamelCase, e.g. `marshal_indent` -> MarshalIndent,
//! `html_escape` -> HTMLEscape), pinned go1.26.9:
//!   * 1 arg: marshal, valid, new_decoder, new_encoder
//!   * 2 args: unmarshal, compact, html_escape
//!   * 3 args: marshal_indent
//!   * 4 args: indent
//!   * Non-callable package types/errors: decoder, encoder, raw_message, delim,
//!     number, marshaler, unmarshaler, syntax_error, unmarshal_type_error,
//!     unsupported_type_error, unsupported_value_error, marshaler_error.
//! Spread calls (`...`) on non-variadic members are strictly guarded.

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
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for {:?}, got {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject, "verdict for {:?}", src);
}

fn accepts(src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "verdict for {:?}: {:?}", src, r.diagnostics);
}

#[test]
fn json_one_arg_members_accept_exact_arity() {
    accepts("import \"encoding/json\"\nfn main() {\n    b, err := json.marshal(\"hello\")\n    _ = b\n    _ = err\n}\n");
    accepts("import \"json\"\nfn main() {\n    b, err := json.marshal(42)\n    _ = b\n    _ = err\n}\n");
    accepts("import \"encoding/json\"\nfn main() {\n    ok := json.valid([]byte(\"[1, 2]\"))\n    _ = ok\n}\n");
    accepts("import \"encoding/json\"\nfn main() {\n    d := json.new_decoder(nil)\n    _ = d\n}\n");
    accepts("import \"encoding/json\"\nfn main() {\n    e := json.new_encoder(nil)\n    _ = e\n}\n");
}

#[test]
fn json_two_arg_members_accept_exact_arity() {
    accepts("import \"encoding/json\"\nfn main() {\n    var x string\n    err := json.unmarshal([]byte(\"\\\"hi\\\"\"), &x)\n    _ = err\n}\n");
    accepts("import \"encoding/json\"\nfn main() {\n    err := json.compact(nil, []byte(\"  [1, 2]  \"))\n    _ = err\n}\n");
    accepts("import \"encoding/json\"\nfn main() {\n    json.html_escape(nil, []byte(\"<tag>\"))\n}\n");
}

#[test]
fn json_three_and_four_arg_members_accept_exact_arity() {
    accepts("import \"encoding/json\"\nfn main() {\n    b, err := json.marshal_indent(42, \"\", \"  \")\n    _ = b\n    _ = err\n}\n");
    accepts("import \"encoding/json\"\nfn main() {\n    err := json.indent(nil, []byte(\"[1, 2]\"), \"\", \"  \")\n    _ = err\n}\n");
}

#[test]
fn json_one_arg_members_reject_wrong_arity() {
    rejects_e1002("import \"encoding/json\"\nfn main() {\n    _ = json.marshal()\n}\n");
    rejects_e1002("import \"encoding/json\"\nfn main() {\n    _ = json.marshal(1, 2)\n}\n");
    rejects_e1002("import \"encoding/json\"\nfn main() {\n    _ = json.valid()\n}\n");
    rejects_e1002("import \"encoding/json\"\nfn main() {\n    _ = json.valid([]byte(\"[1, 2]\"), 1)\n}\n");
    rejects_e1002("import \"encoding/json\"\nfn main() {\n    _ = json.new_decoder()\n}\n");
    rejects_e1002("import \"encoding/json\"\nfn main() {\n    _ = json.new_encoder()\n}\n");
}

#[test]
fn json_multi_arg_members_reject_wrong_arity() {
    rejects_e1002("import \"encoding/json\"\nfn main() {\n    _ = json.unmarshal([]byte(\"1\"))\n}\n");
    rejects_e1002("import \"encoding/json\"\nfn main() {\n    _ = json.unmarshal([]byte(\"1\"), nil, 3)\n}\n");
    rejects_e1002("import \"encoding/json\"\nfn main() {\n    _ = json.compact(nil)\n}\n");
    rejects_e1002("import \"encoding/json\"\nfn main() {\n    _ = json.html_escape(nil)\n}\n");
    rejects_e1002("import \"encoding/json\"\nfn main() {\n    _ = json.marshal_indent(42, \"\")\n}\n");
    rejects_e1002("import \"encoding/json\"\nfn main() {\n    _ = json.indent(nil, []byte(\"[1, 2]\"))\n}\n");
}

#[test]
fn json_package_types_and_errors_are_not_callable() {
    rejects_e1002("import \"encoding/json\"\nfn main() {\n    _ = json.decoder()\n}\n");
    rejects_e1002("import \"encoding/json\"\nfn main() {\n    _ = json.encoder()\n}\n");
    rejects_e1002("import \"encoding/json\"\nfn main() {\n    _ = json.raw_message()\n}\n");
    rejects_e1002("import \"encoding/json\"\nfn main() {\n    _ = json.delim()\n}\n");
    rejects_e1002("import \"encoding/json\"\nfn main() {\n    _ = json.number()\n}\n");
    rejects_e1002("import \"encoding/json\"\nfn main() {\n    _ = json.marshaler()\n}\n");
    rejects_e1002("import \"encoding/json\"\nfn main() {\n    _ = json.unmarshaler()\n}\n");
    rejects_e1002("import \"encoding/json\"\nfn main() {\n    _ = json.syntax_error()\n}\n");
    rejects_e1002("import \"encoding/json\"\nfn main() {\n    _ = json.unmarshal_type_error()\n}\n");
    rejects_e1002("import \"encoding/json\"\nfn main() {\n    _ = json.unsupported_type_error()\n}\n");
    rejects_e1002("import \"encoding/json\"\nfn main() {\n    _ = json.unsupported_value_error()\n}\n");
    rejects_e1002("import \"encoding/json\"\nfn main() {\n    _ = json.marshaler_error()\n}\n");
}

#[test]
fn json_spread_is_rejected_on_non_variadic_calls() {
    rejects_e1002("import \"encoding/json\"\nfn main() {\n    xs := [\"hi\"]\n    _ = json.marshal(xs...)\n}\n");
    rejects_e1002("import \"encoding/json\"\nfn main() {\n    xs := [\"a\", \"b\"]\n    _ = json.unmarshal(xs...)\n}\n");
}

#[test]
fn json_unrelated_local_calls_and_shadowing_still_work() {
    accepts("fn main() {\n    json := fn(x int) int { return x + 1 }\n    v := json(5)\n    _ = v\n}\n");
    accepts("import \"strconv\"\nfn main() {\n    s := strconv.itoa(42)\n    _ = s\n}\n");
}

#[test]
fn json_arity_error_message_is_actionable() {
    let r = check("import \"encoding/json\"\nfn main() {\n    _ = json.marshal_indent(42, \"\")\n}\n");
    let msg = format!("{:?}", r.diagnostics);
    assert!(msg.contains("takes exactly 3 arguments"), "message must state expected arity: {}", msg);
    assert!(msg.contains("got 2"), "message must state actual count: {}", msg);
    assert!(msg.contains("MarshalIndent"), "message must name the Go lowering: {}", msg);
}
