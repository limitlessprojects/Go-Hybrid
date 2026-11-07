//! Regression tests for rejecting elided struct literals (`{{}}` / `{{k: v}}`)
//! on non-struct curated foreign types (`json.number`, `json.delim`, `time.month`,
//! `net.error`, `net.ip{{x: 1}}`, etc.) in `elided_literal.rs` (`HYB-E1002`, Row 13).

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

#[test]
fn test_non_struct_scalar_and_interface_foreign_types_reject_elided_lit() {
    for src in [
        "import \"encoding/json\"\nfn main() { a := []json.number{{}}; _ = a }\n",
        "import \"encoding/json\"\nfn main() { a := []json.delim{{}}; _ = a }\n",
        "import \"encoding/json\"\nfn main() { a := []json.marshaler{{}}; _ = a }\n",
        "import \"time\"\nfn main() { a := []time.month{{}}; _ = a }\n",
        "import \"time\"\nfn main() { a := []time.weekday{{}}; _ = a }\n",
        "import \"net\"\nfn main() { a := []net.error{{}}; _ = a }\n",
        "import \"net\"\nfn main() { a := []net.invalid_addr_error{{}}; _ = a }\n",
        "import \"encoding/hex\"\nfn main() { a := []hex.invalid_byte_error{{}}; _ = a }\n",
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{}", src);
        assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    }
}

#[test]
fn test_slice_and_map_backed_foreign_types_reject_field_keyed_elided_lit() {
    for src in [
        "import \"net\"\nfn main() { a := []net.ip{{x: 1}}; _ = a }\n",
        "import \"encoding/json\"\nfn main() { a := []json.raw_message{{x: 1}}; _ = a }\n",
        "import \"net/http\"\nfn main() { a := []http.header{{x: 1}}; _ = a }\n",
    ] {
        let r = check(src);
        assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{}", src);
        assert!(error_codes(&r).contains(&"HYB-E1002".to_string()));
    }
}

#[test]
fn test_valid_empty_slice_and_struct_foreign_types_accept_elided_lit() {
    let src = r#"
import "net"
import "time"

fn main() {
    ips := []net.ip{{}}
    times := []time.time{{}}
    _ = ips
    _ = times
}
"#;
    let r = check(src);
    assert!(error_codes(&r).is_empty(), "got: {:?}", r.diagnostics);
    assert_eq!(r.verdict, Verdict::Accept);
}