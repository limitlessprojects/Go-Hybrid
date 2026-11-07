use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn test_stdlib_initialisms_and_os_flags_issue_3538() {
    let src = r#"
package main

import "os"
import "net"
import "time"

fn main() {
    flag := os.o_rdonly
    _ = flag
    _ = os.o_append
    _ = os.o_create
    _ = net.ipv4len
    _ = net.ipv6len
    _ = time.utc
    _ = time.ansic
}
"#;
    let ast = parse_file("test.hy", src).expect("parses");
    let emitted = emit_go(&ast).expect("emits");

    assert!(emitted.contains("os.O_RDONLY"), "expected os.O_RDONLY in emitted code, got:\n{emitted}");
    assert!(emitted.contains("os.O_APPEND"), "expected os.O_APPEND in emitted code, got:\n{emitted}");
    assert!(emitted.contains("os.O_CREATE"), "expected os.O_CREATE in emitted code, got:\n{emitted}");
    assert!(emitted.contains("net.IPv4len"), "expected net.IPv4len in emitted code, got:\n{emitted}");
    assert!(emitted.contains("net.IPv6len"), "expected net.IPv6len in emitted code, got:\n{emitted}");
    assert!(emitted.contains("time.UTC"), "expected time.UTC in emitted code, got:\n{emitted}");
    assert!(emitted.contains("time.ANSIC"), "expected time.ANSIC in emitted code, got:\n{emitted}");
}
