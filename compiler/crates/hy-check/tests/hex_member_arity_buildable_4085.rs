//! Regression tests for #4085:
//! The full `encoding/hex` call surface must be arity-checked at `hyb check`
//! time. Only `hex.encode` was checked; every other member the member table
//! in foreign.rs blesses (`encode_to_string`, `decode`, `decode_string`,
//! `dump`, `dumper`, `new_encoder`, `new_decoder`, `encoded_len`,
//! `decoded_len`) escaped with any arity and blew up at `go build` instead —
//! exactly the Row 13 BUILDABLE escape (reject fail-closed, HYB-E1002).
//!
//! Arities mirror the Go std signatures each member lowers to via
//! `to_go_name` (snake -> CamelCase; `encode` is special-cased to
//! `EncodeToString` in hy-emit):
//!   * 1 arg: encode/EncodeToString, encode_to_string, decode_string
//!     (DecodeString, 2 results), dump, dumper, encoded_len, decoded_len,
//!     new_encoder, new_decoder
//!   * 2 args: decode (Decode(dst, src))
//!   * `err_length` and `invalid_byte_error` are package-level values/types,
//!     not functions — calling them must reject.
//! No valid call may be rejected (never over-reject; Go accepts all of these).

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
fn hex_one_arg_members_accept_exact_arity() {
    accepts("import \"encoding/hex\"\nfn main() {\n    b := []byte(\"hi\")\n    s := hex.encode_to_string(b)\n    _ = s\n}\n");
    accepts("import \"encoding/hex\"\nfn main() {\n    s := \"6869\"\n    b, err := hex.decode_string(s)\n    _ = b\n    _ = err\n}\n");
    accepts("import \"encoding/hex\"\nfn main() {\n    b := []byte(\"hi\")\n    s := hex.dump(b)\n    _ = s\n}\n");
    accepts("import \"encoding/hex\"\nfn main() {\n    n := hex.encoded_len(5)\n    _ = n\n}\n");
    accepts("import \"encoding/hex\"\nfn main() {\n    n := hex.decoded_len(2)\n    _ = n\n}\n");
}

#[test]
fn hex_two_arg_members_accept_exact_arity() {
    accepts("import \"encoding/hex\"\nfn main() {\n    dst := make([]byte, 32)\n    src := []byte(\"hi\")\n    n := hex.decode(dst, src)\n    _ = n\n}\n");
}

#[test]
fn hex_one_arg_members_reject_wrong_arity() {
    rejects_e1002("import \"encoding/hex\"\nfn main() {\n    s := hex.encode_to_string()\n    _ = s\n}\n");
    rejects_e1002("import \"encoding/hex\"\nfn main() {\n    b := []byte(\"hi\")\n    s := hex.encode_to_string(b, b)\n    _ = s\n}\n");
    rejects_e1002("import \"encoding/hex\"\nfn main() {\n    s := hex.decode_string()\n    _ = s\n}\n");
    rejects_e1002("import \"encoding/hex\"\nfn main() {\n    s := \"6869\"\n    b, err := hex.decode_string(s, s)\n    _ = b\n    _ = err\n}\n");
    rejects_e1002("import \"encoding/hex\"\nfn main() {\n    s := hex.dump()\n    _ = s\n}\n");
    rejects_e1002("import \"encoding/hex\"\nfn main() {\n    n := hex.encoded_len()\n    _ = n\n}\n");
    rejects_e1002("import \"encoding/hex\"\nfn main() {\n    s := \"6869\"\n    n := hex.decoded_len(s, s)\n    _ = n\n}\n");
}

#[test]
fn hex_two_arg_members_reject_wrong_arity() {
    rejects_e1002("import \"encoding/hex\"\nfn main() {\n    dst := make([]byte, 32)\n    src := []byte(\"hi\")\n    n := hex.decode(src)\n    _ = n\n}\n");
    rejects_e1002("import \"encoding/hex\"\nfn main() {\n    dst := make([]byte, 32)\n    src := []byte(\"hi\")\n    n := hex.decode(dst, src, src)\n    _ = n\n}\n");
}

#[test]
fn hex_error_values_are_not_callable() {
    rejects_e1002("import \"encoding/hex\"\nfn main() {\n    e := hex.err_length(4)\n    _ = e\n}\n");
    rejects_e1002("import \"encoding/hex\"\nfn main() {\n    e := hex.invalid_byte_error(0x7f)\n    _ = e\n}\n");
}

#[test]
fn hex_encode_regression_3863_still_holds() {
    // pre-existing behavior kept: encode() zero-arg and two-arg reject,
    // encode(data) with one slice arg accepts.
    rejects_e1002("import \"encoding/hex\"\nfn main() {\n    s := hex.encode()\n    _ = s\n}\n");
    rejects_e1002("import \"encoding/hex\"\nfn main() {\n    b := []byte(\"hi\")\n    s := hex.encode(b, b)\n    _ = s\n}\n");
    accepts("import \"encoding/hex\"\nfn main() {\n    b := []byte(\"hi\")\n    s := hex.encode(b)\n    _ = s\n}\n");
}

#[test]
fn hex_non_variadic_spread_rejected_e1002() {
    let cases = [
        "import \"encoding/hex\"\nfn main() {\n    var xs = [[]byte(\"hi\")]\n    s := hex.encode(xs...)\n    _ = s\n}\n",
        "import \"encoding/hex\"\nfn main() {\n    var xs = [[]byte(\"hi\")]\n    s := hex.encode_to_string(xs...)\n    _ = s\n}\n",
        "import \"encoding/hex\"\nfn main() {\n    var xs = [\"6869\"]\n    b, err := hex.decode_string(xs...)\n    _ = b\n    _ = err\n}\n",
        "import \"encoding/hex\"\nfn main() {\n    var xs = [[]byte(\"hi\")]\n    s := hex.dump(xs...)\n    _ = s\n}\n",
        "import \"encoding/hex\"\nfn main() {\n    var xs = [5]\n    n := hex.encoded_len(xs...)\n    _ = n\n}\n",
        "import \"encoding/hex\"\nfn main() {\n    var xs = [2]\n    n := hex.decoded_len(xs...)\n    _ = n\n}\n",
        "import \"encoding/hex\"\nfn main() {\n    dst := make([]byte, 32)\n    var xs = [[]byte(\"hi\")]\n    n := hex.decode(dst, xs...)\n    _ = n\n}\n",
    ];
    for src in cases {
        rejects_e1002(src);
    }
}

#[test]
fn time_duration_constructors_reject_variadic_spread_e1002() {
    let cases = [
        "import \"time\"\nfn main() {\n    var ns = [10]\n    d := time.milliseconds(ns...)\n    _ = d\n}\n",
        "import \"time\"\nfn main() {\n    var ns = [10]\n    d := time.seconds(ns...)\n    _ = d\n}\n",
        "import \"time\"\nfn main() {\n    var ns = [10]\n    d := time.minutes(ns...)\n    _ = d\n}\n",
        "import \"time\"\nfn main() {\n    var ns = [10]\n    d := time.hours(ns...)\n    _ = d\n}\n",
    ];
    for src in cases {
        rejects_e1002(src);
    }
    accepts("import \"time\"\nfn main() {\n    d := time.seconds(10)\n    _ = d\n}\n");
}

#[test]
fn io_non_variadic_spread_rejected_e1002() {
    let cases = [
        "import \"io\"\nfn main() {\n    var rs []io.reader\n    b, err := io.read_all(rs...)\n    _ = b\n    _ = err\n}\n",
        "import \"io\"\nfn main() {\n    var w io.writer\n    var rs []io.reader\n    n, err := io.copy(w, rs...)\n    _ = n\n    _ = err\n}\n",
        "import \"io\"\nfn main() {\n    var r io.reader\n    var bufs [][]byte\n    n, err := io.read_full(r, bufs...)\n    _ = n\n    _ = err\n}\n",
        "import \"io\"\nfn main() {\n    var w io.writer\n    var ss = [\"hi\"]\n    n, err := io.write_string(w, ss...)\n    _ = n\n    _ = err\n}\n",
        "import \"io\"\nfn main() {\n    var r io.reader\n    var ns = [int64(10)]\n    lr := io.limit_reader(r, ns...)\n    _ = lr\n}\n",
        "import \"io\"\nfn main() {\n    var r io.reader\n    var ws []io.writer\n    tr := io.tee_reader(r, ws...)\n    _ = tr\n}\n",
    ];
    for src in cases {
        rejects_e1002(src);
    }
    accepts("import \"io\"\nfn main() {\n    var rs []io.reader\n    mr := io.multi_reader(rs...)\n    _ = mr\n}\n");
    accepts("import \"io\"\nfn main() {\n    var ws []io.writer\n    mw := io.multi_writer(ws...)\n    _ = mw\n}\n");
}

