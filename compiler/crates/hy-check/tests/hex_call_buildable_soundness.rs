//! Regression tests for issue #4218: `encoding/hex` curated package call arity
//! and non-callable `hex.err_length` rejection (`HYB-E1002`, GUARANTEES.md Row 13 BUILDABLE),
//! aligned with the SPEC M5 1-arg `hex.encode(src)` curated surface.

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

#[test]
fn test_hex_encode_to_string_arity_rejects() {
    rejects_e1002(
        r#"
package main
import "encoding/hex"
fn main() {
    s := hex.encode_to_string()
    _ = s
}
"#,
    );
    rejects_e1002(
        r#"
package main
import "encoding/hex"
fn main() {
    s := hex.encode_to_string("a", "b")
    _ = s
}
"#,
    );
}

#[test]
fn test_hex_decode_string_arity_rejects() {
    rejects_e1002(
        r#"
package main
import "encoding/hex"
fn main() {
    b, err := hex.decode_string()
    _ = b
    _ = err
}
"#,
    );
    rejects_e1002(
        r#"
package main
import "encoding/hex"
fn main() {
    b, err := hex.decode_string("a", "b")
    _ = b
    _ = err
}
"#,
    );
}

#[test]
fn test_hex_encode_m5_curated_one_arg_arity_rejects() {
    // SPEC M5 curated surface: `hex.encode(src)` takes 1 argument (lowers to EncodeToString).
    rejects_e1002(
        r#"
package main
import "encoding/hex"
fn main() {
    s := hex.encode()
    _ = s
}
"#,
    );
    rejects_e1002(
        r#"
package main
import "encoding/hex"
fn main() {
    s := hex.encode("dst", "src")
    _ = s
}
"#,
    );
    rejects_e1002(
        r#"
package main
import "encoding/hex"
fn main() {
    s := hex.encode("a", "b", "c")
    _ = s
}
"#,
    );
}

#[test]
fn test_hex_decode_arity_rejects() {
    rejects_e1002(
        r#"
package main
import "encoding/hex"
fn main() {
    n, err := hex.decode()
    _ = n
    _ = err
}
"#,
    );
    rejects_e1002(
        r#"
package main
import "encoding/hex"
fn main() {
    n, err := hex.decode("only_one")
    _ = n
    _ = err
}
"#,
    );
}

#[test]
fn test_hex_encoded_len_arity_rejects() {
    rejects_e1002(
        r#"
package main
import "encoding/hex"
fn main() {
    l := hex.encoded_len()
    _ = l
}
"#,
    );
    rejects_e1002(
        r#"
package main
import "encoding/hex"
fn main() {
    l := hex.encoded_len(10, 20)
    _ = l
}
"#,
    );
}

#[test]
fn test_hex_decoded_len_arity_rejects() {
    rejects_e1002(
        r#"
package main
import "encoding/hex"
fn main() {
    l := hex.decoded_len()
    _ = l
}
"#,
    );
    rejects_e1002(
        r#"
package main
import "encoding/hex"
fn main() {
    l := hex.decoded_len(10, 20)
    _ = l
}
"#,
    );
}

#[test]
fn test_hex_dump_and_dumper_arity_rejects() {
    rejects_e1002(
        r#"
package main
import "encoding/hex"
fn main() {
    s := hex.dump()
    _ = s
}
"#,
    );
    rejects_e1002(
        r#"
package main
import "encoding/hex"
fn main() {
    w := hex.dumper()
    _ = w
}
"#,
    );
}

#[test]
fn test_hex_decoder_and_encoder_arity_rejects() {
    rejects_e1002(
        r#"
package main
import "encoding/hex"
fn main() {
    r := hex.new_decoder()
    _ = r
}
"#,
    );
    rejects_e1002(
        r#"
package main
import "encoding/hex"
fn main() {
    w := hex.new_encoder()
    _ = w
}
"#,
    );
}

#[test]
fn test_hex_err_length_non_callable_rejects() {
    rejects_e1002(
        r#"
package main
import "encoding/hex"
fn main() {
    _ = hex.err_length()
}
"#,
    );
}

#[test]
fn test_hex_invalid_named_parameter_rejects() {
    rejects_e1002(
        r#"
package main
import "encoding/hex"
fn main() {
    s := hex.encode_to_string(payload="hello")
    _ = s
}
"#,
    );
    rejects_e1002(
        r#"
package main
import "encoding/hex"
fn main() {
    s := hex.encode(target="hello")
    _ = s
}
"#,
    );
}

#[test]
fn test_hex_shadowed_package_preserves_user_semantics() {
    accepts(
        r#"
package main
struct my_hex {
    val int
}

fn (h my_hex) encode_to_string() int {
    return h.val
}

fn main() {
    hex := my_hex{val: 42}
    x := hex.encode_to_string()
    _ = x
}
"#,
    );
}

#[test]
fn test_hex_valid_calls_including_m5_one_arg_encode_accept() {
    accepts(
        r#"
package main
import "encoding/hex"
fn main() {
    e1 := hex.encode("hello")
    _ = e1
    e2 := hex.encode(src="world")
    _ = e2
    s := hex.encode_to_string("hello")
    _ = s
    l := hex.encoded_len(16)
    _ = l
    dl := hex.decoded_len(32)
    _ = dl
    s2 := hex.encode_to_string(src="world")
    _ = s2
}
"#,
    );
}
