//! Tests for issue #2203 / #2200 -- unknown field names on foreign handle values.
//!
//! `hyb check` accepted field selections on resolved foreign handles -- read or
//! write, in value or assignment position -- and `go build` rejected them with
//! `... undefined (type <T> has no field or method <Name>)`, violating
//! GUARANTEES.md Row 13 (BUILDABLE).

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("parses");
    check_file(&ast)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn assert_code(src: &str, want: &str) {
    let cs = codes(src);
    assert!(
        !cs.is_empty() && cs.iter().all(|c| c == want),
        "expected only [{want}], got {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(cs.is_empty(), "expected 0 diagnostics, got {cs:?} for:\n{src}");
}

#[test]
fn field_read_on_serde_json_value_is_rejected() {
    assert_code(
        r#"
import "rust/serde_json"
import "fmt"

fn main() {
    doc := serde_json.parse(`{}`)?
    fmt.println("v={doc.totally_fake_field}")
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn field_write_on_serde_json_value_is_rejected() {
    assert_code(
        r#"
import "rust/serde_json"

fn main() {
    doc := serde_json.parse(`{}`)?
    doc.totally_fake_field = 1
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn field_read_on_opaque_handle_struct_field_is_rejected() {
    assert_code(
        r#"
import "sync"
import "fmt"

struct stats {
    mu sync.mutex
    n int
}

fn main() {
    var s stats
    fmt.println("m={s.mu.fakefield}")
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn field_write_on_opaque_handle_struct_field_is_rejected() {
    assert_code(
        r#"
import "sync"

struct stats {
    mu sync.mutex
    n int
}

fn main() {
    var s stats
    s.mu.fakefield = 1
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn field_read_on_direct_opaque_handle_is_rejected() {
    assert_code(
        r#"
import "sync"
import "fmt"

fn main() {
    var mu sync.mutex
    fmt.println("m={mu.fakefield}")
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn field_write_on_direct_opaque_handle_is_rejected() {
    assert_code(
        r#"
import "sync"

fn main() {
    var mu sync.mutex
    mu.fakefield = 1
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn positive_control_handle_usage_passes() {
    assert_ok(
        r#"
import "rust/serde_json"
import "fmt"

fn main() {
    doc := serde_json.parse(`{}`)?
    a := doc.array("items")?
    fmt.println("n={len(a)}")
}
"#,
    );
}

#[test]
fn positive_control_struct_with_handle_passes() {
    assert_ok(
        r#"
import "sync"
import "fmt"

struct stats {
    mu sync.mutex
    n int
}

fn main() {
    var s stats
    s.mu.lock()
    s.n = 5
    s.mu.unlock()
    fmt.println("n={s.n}")
}
"#,
    );
}
