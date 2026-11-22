use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn assert_literal(prefix: &str, ty: &str, literal: &str, expected: Verdict) {
    let src = format!("{prefix}\nfn main() {{ xs := []{ty}{{{literal}}}\n _ = xs }}");
    let ast = parse_file("t.hy", &src).unwrap_or_else(|d| panic!("{src}: {d:?}"));
    let r = check_file(&ast);
    assert_eq!(r.verdict, expected, "{src}: {:?}", r.diagnostics);
    if expected == Verdict::Reject {
        assert!(
            r.diagnostics.iter().any(|d| d.severity == Severity::Error && d.code == "HYB-E1002"),
            "expected HYB-E1002 for {src}: {:?}", r.diagnostics
        );
    }
}

#[test]
fn scalar_interface_and_function_slots_reject_both_literal_forms() {
    for (pkg, types) in [
        ("encoding/json", &["json.number", "json.delim", "json.token", "json.marshaler", "json.unmarshaler"][..]),
        ("time", &["time.month", "time.weekday"][..]),
        ("net", &["net.error", "net.flags", "net.invalid_addr_error", "net.unknown_network_error"][..]),
        ("net/url", &["url.escape_error", "url.invalid_host_error"][..]),
        ("encoding/hex", &["hex.invalid_byte_error"][..]),
        ("sync", &["sync.locker"][..]),
        ("net/http", &["http.handler_func", "http.dir", "http.conn_state", "http.same_site", "http.cookie_jar", "http.file", "http.file_system", "http.close_notifier", "http.pusher"][..]),
        ("path/filepath", &["filepath.walk_func"][..]),
        ("bufio", &["bufio.split_func"][..]),
    ] {
        let import = format!("import \"{pkg}\"");
        for ty in types {
            for literal in ["{}", "{x: 1}"] {
                assert_literal(&import, ty, literal, Verdict::Reject);
            }
        }
    }
}

#[test]
fn foreign_slice_and_map_slots_allow_only_the_empty_form() {
    for (pkg, ty) in [
        ("net", "net.ip"),
        ("net", "net.ip_mask"),
        ("net", "net.hardware_addr"),
        ("net", "net.buffers"),
        ("encoding/json", "json.raw_message"),
        ("sort", "sort.int_slice"),
        ("sort", "sort.float64_slice"),
        ("sort", "sort.string_slice"),
        ("net/http", "http.header"),
        ("net/url", "url.values"),
    ] {
        let import = format!("import \"{pkg}\"");
        assert_literal(&import, ty, "{}", Verdict::Accept);
        assert_literal(&import, ty, "{x: 1}", Verdict::Reject);
    }
}

#[test]
fn struct_slots_still_accept_elided_literals() {
    assert_literal("import \"net/url\"", "url.url", "{}", Verdict::Accept);
    assert_literal("struct point { x int }", "point", "{x: 1}", Verdict::Accept);
}
