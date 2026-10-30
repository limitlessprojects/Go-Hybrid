//! Tests for issue #2200 — unknown method names on foreign values.
//!
//! `hyb check` accepted unknown method calls on foreign runtime values —
//! e.g. `doc.totally_fake_method("x")?` or `doc.chanset("pruned", true)?` on
//! `serde_json.value` — and `go build` subsequently failed with `... undefined
//! (type <T> has no field or method <Name>)`, violating GUARANTEES.md Row 13 (BUILDABLE).

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
fn method_call_on_serde_json_value_unknown_method_is_rejected() {
    assert_code(
        r#"
import "rust/serde_json"

fn main() {
    doc := serde_json.parse("[1]")?
    doc.totally_fake_method("x")?
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn method_call_on_serde_json_value_chanset_is_rejected() {
    assert_code(
        r#"
import "rust/serde_json"

fn main() {
    doc := serde_json.parse("[1]")?
    doc.chanset("pruned", true)?
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn method_call_on_direct_opaque_handle_unknown_method_is_rejected() {
    assert_code(
        r#"
import "sync"

fn main() {
    var mu sync.mutex
    mu.fake_method()
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn method_call_on_struct_field_opaque_handle_unknown_method_is_rejected() {
    assert_code(
        r#"
import "sync"

struct stats {
    mu sync.mutex
    n int
}

fn main() {
    var s stats
    s.mu.fake_method()
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn method_call_on_atomic_unknown_method_is_rejected() {
    assert_code(
        r#"
import "sync/atomic"

fn main() {
    var hits atomic.int64
    hits.fake_method()
}
"#,
        "HYB-E1002",
    );
}

#[test]
fn positive_control_serde_json_methods_pass() {
    assert_ok(
        r#"
import "rust/serde_json"
import "fmt"

fn main() {
    doc := serde_json.parse("[1]")?
    doc.set("k", 1)?
    a := doc.array("items")?
    fmt.println("n={len(a)}")
}
"#,
    );
}

#[test]
fn positive_control_sync_mutex_methods_pass() {
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

#[test]
fn positive_control_atomic_methods_pass() {
    assert_ok(
        r#"
import "sync/atomic"
import "fmt"

fn main() {
    var hits atomic.int64
    hits.add(1)
    v := hits.load()
    fmt.println("v={v}")
}
"#,
    );
}
