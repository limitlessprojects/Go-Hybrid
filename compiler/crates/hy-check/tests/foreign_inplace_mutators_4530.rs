//! foreign_inplace_mutators_4530.rs — regression tests for issue #4530:
//! in-place foreign mutators (hex.decode, Reader.read) must model argument stores
//! so passing a const root rejects with HYB-E3006.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn hex_decode_into_const_is_e3006() {
    let src = r#"
import "encoding/hex"

fn main() {
    const dst = []byte{0, 0}
    src := []byte("6869")
    _ = hex.decode(dst, src)
}
"#;
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for hex.decode into const, got {:?}",
        cs
    );
}

#[test]
fn hex_encode_into_const_is_e3006() {
    let src = r#"
import "encoding/hex"

fn main() {
    const dst = []byte{0, 0, 0, 0}
    src := []byte("hi")
    _ = hex.encode(dst, src)
}
"#;
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for hex.encode into const, got {:?}",
        cs
    );
}

#[test]
fn reader_read_into_const_is_e3006() {
    let src = r#"
import "strings"

fn main() {
    const dst = []byte{0, 0}
    r := strings.new_reader("hello")
    _ = r.read(dst)
}
"#;
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for reader.read into const, got {:?}",
        cs
    );
}

#[test]
fn reader_read_at_into_const_is_e3006() {
    let src = r#"
import "strings"

fn main() {
    const dst = []byte{0, 0}
    r := strings.new_reader("hello")
    _ = r.read_at(dst, 0)
}
"#;
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for reader.read_at into const, got {:?}",
        cs
    );
}

#[test]
fn bufio_reader_read_into_const_is_e3006() {
    let src = r#"
import "bufio"
import "strings"

fn main() {
    const dst = []byte{0, 0}
    r := bufio.new_reader(strings.new_reader("hello"))
    _ = r.read(dst)
}
"#;
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for bufio.reader.read into const, got {:?}",
        cs
    );
}

#[test]
fn os_open_try_read_into_const_is_e3006() {
    let src = r#"
import "os"

fn run() error {
    const dst = []byte{0, 0}
    f := os.open("test.txt")?
    _ = f.read(dst)
    return nil
}
"#;
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for os.open?.read into const, got {:?}",
        cs
    );
}

#[test]
fn bytes_buffer_read_into_const_is_e3006() {
    let src = r#"
import "bytes"

fn main() {
    const dst = []byte{0, 0}
    b := bytes.new_buffer([]byte("hello"))
    _ = b.read(dst)
}
"#;
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for bytes.buffer.read into const, got {:?}",
        cs
    );
}

#[test]
fn hex_decode_into_mutable_local_accepted() {
    let src = r#"
import "encoding/hex"

fn main() {
    dst := make([]byte, 2)
    src := []byte("6869")
    _ = hex.decode(dst, src)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn hex_decode_string_read_only_accepted() {
    let src = r#"
import "encoding/hex"

fn main() {
    const src = "6869"
    _, _ = hex.decode_string(src)
}
"#;
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept);
}
