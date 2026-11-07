use hy_check::{Verdict, check_file};
use hy_syntax::parse_file;

#[test]
fn curated_multi_return_stdlib_calls_reject_arity_mismatch_with_e2002() {
    for src in [
        "import \"net/url\"\nfn main() { u := url.parse(\"https://example.com\")\n _ = u }",
        "import \"net/url\"\nfn main() { q := url.parse_query(\"a=1\")\n _ = q }",
        "import \"encoding/json\"\nfn main() { b := json.marshal(1)\n _ = b }",
        "import \"net/http\"\nfn main() { r := http.get(\"https://example.com\")\n _ = r }",
        "import \"net/http\"\nfn main() { a, b := http.parse_http_version(\"HTTP/1.1\")\n _ = a\n _ = b }",
        "import \"os\"\nfn main() { f := os.open_file(\"a\", os.o_rdonly, 0)\n _ = f }",
        "import \"os\"\nfn main() { r, w := os.pipe()\n _ = r\n _ = w }",
        "import \"net\"\nfn main() { ip, n := net.parse_cidr(\"10.0.0.0/8\")\n _ = ip\n _ = n }",
    ] {
        let r = check_file(&parse_file("test.hy", src).expect("parse"));
        assert!(
            r.verdict == Verdict::Reject && r.diagnostics.iter().any(|d| d.code == "HYB-E2002"),
            "expected HYB-E2002 for:\n{src}\ngot: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn curated_multi_return_stdlib_calls_accept_matching_arity_or_try() {
    for src in [
        "import \"net/url\"\nfn main() error { u := url.parse(\"https://example.com\")?\n _ = u\n return nil }",
        "import \"encoding/json\"\nfn main() { b, err := json.marshal(1)\n _ = b\n _ = err }",
        "import \"net/http\"\nfn main() { maj, min, ok := http.parse_http_version(\"HTTP/1.1\")\n _ = maj\n _ = min\n _ = ok }",
        "import \"os\"\nfn main() { r, w, err := os.pipe()\n _ = r\n _ = w\n _ = err }",
        "import \"net\"\nfn main() { ip, n, err := net.parse_cidr(\"10.0.0.0/8\")\n _ = ip\n _ = n\n _ = err }",
    ] {
        let r = check_file(&parse_file("test.hy", src).expect("parse"));
        assert!(
            r.verdict == Verdict::Accept && r.diagnostics.is_empty(),
            "expected Accept for:\n{src}\ngot: {:?}",
            r.diagnostics
        );
    }
}
