use hy_emit::emit_go;
use hy_syntax::parse_file;
use std::process::Command;

#[test]
fn curated_stdlib_initialisms_and_os_open_flags_emit_and_run() {
    let src = "\
package main
import \"fmt\"
import \"net\"
import \"net/http\"
import \"net/url\"
import \"os\"
import \"time\"

fn unused_network_refs() {
    _ = os.dir_fs(\".\")
    _ = http.http2_config{}
    _, _ = url.parse(\"https://example.com\")
    _, _, _ = net.parse_cidr(\"10.0.0.1/24\")
    _ = net.cidr_mask(24, 32)
    _, _ = net.parse_mac(\"00:00:5e:00:53:01\")
    _, _ = net.lookup_cname(\"localhost\")
    _, _ = net.lookup_mx(\"localhost\")
    _, _ = net.lookup_ns(\"localhost\")
    _, _, _ = net.lookup_srv(\"\", \"\", \"localhost\")
    _, _ = net.lookup_txt(\"localhost\")
}

fn main() {
    flags := os.o_rdonly | os.o_wronly | os.o_rdwr | os.o_append | os.o_create | os.o_excl | os.o_sync | os.o_trunc
    t := time.date(2026, time.november, 3, 12, 0, 0, 0, time.utc)
    fmt.println(flags != 0, time.ansic != \"\", t.location() == time.utc, net.ipv4len, net.ipv6len)
}
";
    let go_src = emit_go(&parse_file("test.hy", src).expect("parse")).expect("emit");
    for expected in [
        "os.DirFS(\".\")",
        "http.HTTP2Config{}",
        "url.Parse(\"https://example.com\")",
        "net.ParseCIDR(",
        "net.CIDRMask(24, 32)",
        "net.ParseMAC(",
        "net.LookupCNAME(",
        "net.LookupMX(",
        "net.LookupNS(",
        "net.LookupSRV(",
        "net.LookupTXT(",
        "os.O_RDONLY | os.O_WRONLY | os.O_RDWR | os.O_APPEND | os.O_CREATE | os.O_EXCL | os.O_SYNC | os.O_TRUNC",
        "time.UTC",
        "time.ANSIC",
        "net.IPv4len, net.IPv6len",
    ] {
        assert!(go_src.contains(expected), "missing `{expected}` in:\n{go_src}");
    }
    let go_file = std::env::temp_dir().join("hy_curated_stdlib_initialisms_and_flags.go");
    std::fs::write(&go_file, &go_src).expect("write");
    let out = Command::new("go")
        .args(["run", go_file.to_str().unwrap()])
        .output()
        .expect("go run");
    let _ = std::fs::remove_file(&go_file);
    assert!(
        out.status.success(),
        "{}\n---\n{}",
        String::from_utf8_lossy(&out.stderr),
        go_src
    );
    assert_eq!(
        String::from_utf8_lossy(&out.stdout).trim(),
        "true true true 4 16"
    );
}
