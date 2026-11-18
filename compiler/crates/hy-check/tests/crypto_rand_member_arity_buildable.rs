//! Regression tests for crypto/rand call arity and symbol validation:
//! The full `crypto/rand` call surface must be arity-checked at `hyb check`
//! time. Before this, calling rand package members with wrong arity or calling
//! non-callable package-level values/variables (`reader`) passed `hyb check`
//! with any arity and blew up at `go build` instead (Row 13 BUILDABLE: reject
//! fail-closed, HYB-E1002).
//!
//! Arities mirror the Go std signatures each member lowers to via `to_go_name`:
//!   * 0 args: text (Text() in Go 1.24+)
//!   * 1 arg: read (Read(b []byte) (n int, err error))
//!   * 2 args: prime (Prime(rand io.Reader, bits int)), int (Int(rand io.Reader, max *big.Int))
//!   * reader is a package-level variable (io.Reader) — calling it must reject with HYB-E1002.
//!   * None of rand's functions accept variadic spread (`...`).

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
    let cs = error_codes(&r);
    assert!(
        cs.is_empty(),
        "expected accept for {:?}, got errors: {:?}",
        src,
        cs
    );
    assert_eq!(r.verdict, Verdict::Accept, "verdict for {:?}", src);
}

#[test]
fn test_rand_package_variable_reader_call_rejected() {
    let cases = [
        "import \"crypto/rand\"\nfn main() { rand.reader() }",
        "import \"crypto/rand\"\nfn main() { rand.reader(1) }",
        "import \"crypto/rand\"\nfn main() { rand.reader(1, 2) }",
    ];
    for src in cases {
        rejects_e1002(src);
    }
}

#[test]
fn test_rand_read_arity_rejected() {
    let cases = [
        "import \"crypto/rand\"\nfn main() { rand.read() }",
        "import \"crypto/rand\"\nfn main() { b := []byte{}\nrand.read(b, b) }",
        "import \"crypto/rand\"\nfn main() { b := []byte{}\nrand.read(b, 1, 2) }",
    ];
    for src in cases {
        rejects_e1002(src);
    }
}

#[test]
fn test_rand_prime_arity_rejected() {
    let cases = [
        "import \"crypto/rand\"\nfn main() { rand.prime() }",
        "import \"crypto/rand\"\nfn main() { rand.prime(rand.reader) }",
        "import \"crypto/rand\"\nfn main() { rand.prime(rand.reader, 256, 1) }",
    ];
    for src in cases {
        rejects_e1002(src);
    }
}

#[test]
fn test_rand_int_arity_rejected() {
    let cases = [
        "import \"crypto/rand\"\nfn main() { rand.int() }",
        "import \"crypto/rand\"\nfn main() { rand.int(rand.reader) }",
        "import \"crypto/rand\"\nfn main() { rand.int(rand.reader, 10, 20) }",
    ];
    for src in cases {
        rejects_e1002(src);
    }
}

#[test]
fn test_rand_text_arity_rejected() {
    let cases = [
        "import \"crypto/rand\"\nfn main() { rand.text(1) }",
        "import \"crypto/rand\"\nfn main() { rand.text(1, 2) }",
    ];
    for src in cases {
        rejects_e1002(src);
    }
}

#[test]
fn test_rand_spread_rejected() {
    let cases = [
        "import \"crypto/rand\"\nfn main() { xs := []byte{}\nrand.read(xs...) }",
        "import \"crypto/rand\"\nfn main() { xs := []int{256}\nrand.prime(rand.reader, xs...) }",
    ];
    for src in cases {
        rejects_e1002(src);
    }
}

#[test]
fn test_rand_valid_calls_accepted() {
    let cases = [
        "import \"crypto/rand\"\nfn main() {\n  b := []byte{}\n  _, _ = rand.read(b)\n}",
        "import \"crypto/rand\"\nfn main() {\n  _, _ = rand.prime(rand.reader, 256)\n}",
        "import \"crypto/rand\"\nfn main() {\n  _, _ = rand.int(rand.reader, 100)\n}",
        "import \"crypto/rand\"\nfn main() {\n  _ = rand.text()\n}",
        "import \"crypto/rand\"\nfn main() {\n  r := rand.reader\n  _ = r\n}",
    ];
    for src in cases {
        accepts(src);
    }
}
