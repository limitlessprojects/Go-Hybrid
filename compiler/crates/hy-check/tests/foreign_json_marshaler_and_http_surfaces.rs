//! Tests for curated foreign method surfaces of `json.marshaler`, `json.unmarshaler`,
//! `http.cookie_jar`, `http.close_notifier`, `http.dir`, `http.max_bytes_error`,
//! `http.protocol_error`, and `http.protocols` (HYB-E1002, Row 13 BUILDABLE, #4478).

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn phantom_json_marshaler_and_unmarshaler_methods_rejected() {
    let bad = check(
        "import \"encoding/json\"\n\
         struct codec {\n\
             m json.marshaler\n\
             u json.unmarshaler\n\
         }\n\
         fn main() {\n\
             var c codec\n\
             c.m.totally_fake_marshal()\n\
             c.u.totally_fake_unmarshal()\n\
             fmt.println(1)\n\
         }\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject, "expected reject, got {:?}", bad.diagnostics);
    let all_codes = codes(&bad);
    assert!(
        all_codes.iter().filter(|c| *c == "HYB-E1002").count() >= 2,
        "expected multiple HYB-E1002 errors, got {:?}",
        all_codes
    );
}

#[test]
fn phantom_http_cookie_jar_close_notifier_and_dir_methods_rejected() {
    for (ty, fake_call) in [
        ("http.cookie_jar", "jar.phantom_clear()"),
        ("http.close_notifier", "cn.bogus_notify()"),
        ("http.dir", "d.invalid_walk()"),
    ] {
        let var_name = fake_call.split('.').next().unwrap();
        let src = format!(
            "fn check_val({var_name} {ty}) {{\n\
                 {fake_call}\n\
                 fmt.println(1)\n\
             }}\n\
             fn main() {{}}\n"
        );
        let r = check(&src);
        assert_eq!(r.verdict, Verdict::Reject, "expected reject for {ty}, got {:?}", r.diagnostics);
        assert!(
            codes(&r).iter().any(|c| c == "HYB-E1002"),
            "expected HYB-E1002 for {ty}, got {:?}",
            codes(&r)
        );
    }
}

#[test]
fn phantom_http_errors_and_protocols_methods_rejected() {
    for (ty, fake_call) in [
        ("http.max_bytes_error", "mbe.fake_limit()"),
        ("http.protocol_error", "pe.fake_code()"),
        ("http.protocols", "pr.fake_http3()"),
    ] {
        let var_name = fake_call.split('.').next().unwrap();
        let src = format!(
            "fn check_val({var_name} {ty}) {{\n\
                 {fake_call}\n\
                 fmt.println(1)\n\
             }}\n\
             fn main() {{}}\n"
        );
        let r = check(&src);
        assert_eq!(r.verdict, Verdict::Reject, "expected reject for {ty}, got {:?}", r.diagnostics);
        assert!(
            codes(&r).iter().any(|c| c == "HYB-E1002"),
            "expected HYB-E1002 for {ty}, got {:?}",
            codes(&r)
        );
    }
}

#[test]
fn pointer_receivers_enforce_json_and_http_surfaces() {
    let bad = check(
        "import \"encoding/json\"\n\
         fn check_ptrs(m *json.marshaler, pr *http.protocols) {\n\
             m.fake_encode()\n\
             pr.fake_quic()\n\
             fmt.println(1)\n\
         }\n\
         fn main() {}\n",
    );
    assert_eq!(bad.verdict, Verdict::Reject, "expected reject, got {:?}", bad.diagnostics);
    let all_codes = codes(&bad);
    assert!(
        all_codes.iter().filter(|c| *c == "HYB-E1002").count() >= 2,
        "expected multiple HYB-E1002 errors, got {:?}",
        all_codes
    );
}

#[test]
fn real_json_marshaler_and_http_methods_accept_and_preserve_result_shapes() {
    let good = check(
        "import \"encoding/json\"\n\
         struct holder {\n\
             m   json.marshaler\n\
             u   json.unmarshaler\n\
             jar http.cookie_jar\n\
             cn  http.close_notifier\n\
             d   http.dir\n\
             mbe http.max_bytes_error\n\
             pe  http.protocol_error\n\
             pr  http.protocols\n\
         }\n\
         fn main() {\n\
             var h holder\n\
             _ = h.m.marshal_json()\n\
             _ = h.u.unmarshal_json([]byte{})\n\
             _ = h.jar.cookies(nil)\n\
             h.jar.set_cookies(nil, nil)\n\
             _ = h.cn.close_notify()\n\
             _ = h.d.open(\"index.html\")\n\
             s1 := h.mbe.error()\n\
             s2 := h.pe.error()\n\
             b1 := h.pe.is(nil)\n\
             b2 := h.pr.http1()\n\
             b3 := h.pr.http2()\n\
             b4 := h.pr.unencrypted_http2()\n\
             h.pr.set_http1(true)\n\
             h.pr.set_http2(true)\n\
             h.pr.set_unencrypted_http2(false)\n\
             s3 := h.pr.string()\n\
             ch := make(chan string, 3)\n\
             ch <- s1\n\
             ch <- s2\n\
             ch <- s3\n\
             bch := make(chan bool, 4)\n\
             bch <- b1\n\
             bch <- b2\n\
             bch <- b3\n\
             bch <- b4\n\
             fmt.println(<-ch)\n\
             fmt.println(<-bch)\n\
         }\n",
    );
    assert_eq!(good.verdict, Verdict::Accept, "expected accept, got {:?}", good.diagnostics);
}
