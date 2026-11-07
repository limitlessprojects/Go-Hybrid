//! Tests for curated foreign method surface of `http.serve_mux`, `http.header`, and `http.transport` (HYB-E1002, Row 13 BUILDABLE).
//! Guarantees that calling phantom/unsupported methods on these receivers or their pointers
//! rejects fail-closed with HYB-E1002, while all real Go standard library methods accept cleanly.

use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn phantom_http_receivers_methods_are_rejected() {
    for recv in ["m http.serve_mux", "h http.header", "t http.transport"] {
        let var_name = recv.split_whitespace().next().unwrap();
        let src = format!(
            "import \"http\"\n             struct holder {{\n                 {recv}\n             }}\n             fn main() {{\n                 var s holder\n                 s.{var_name}.totally_fake_method()\n                 fmt.println(1)\n             }}\n"
        );
        let r = check(&src);
        assert_eq!(r.verdict, Verdict::Reject, "expected reject for {recv}, got {:?}", r.diagnostics);
        assert!(
            codes(&r).iter().any(|c| c == "HYB-E1002"),
            "expected HYB-E1002 for {recv}, got {:?}",
            codes(&r)
        );
    }
}

#[test]
fn pointer_http_receivers_phantom_methods_are_rejected() {
    for (param, field) in [("m *http.serve_mux", "m"), ("h *http.header", "h"), ("t *http.transport", "t")] {
        let src = format!(
            "import \"http\"\n             fn check_ptr({param}) {{\n                 {field}.unknown_method()\n                 fmt.println(1)\n             }}\n             fn main() {{}}\n"
        );
        let r = check(&src);
        assert_eq!(r.verdict, Verdict::Reject, "expected reject for {param}, got {:?}", r.diagnostics);
        assert!(
            codes(&r).iter().any(|c| c == "HYB-E1002"),
            "expected HYB-E1002 for {param}, got {:?}",
            codes(&r)
        );
    }
}

#[test]
fn real_http_serve_mux_methods_accept() {
    let good = check(
        "import \"http\"\n         struct holder {\n             m http.serve_mux\n         }\n         fn main() {\n             var h holder\n             _ = h.m\n             fmt.println(1)\n         }\n",
    );
    assert_eq!(good.verdict, Verdict::Accept, "expected accept, got {:?}", good.diagnostics);
}

#[test]
fn real_http_header_methods_accept() {
    let good = check(
        "import \"http\"\n         struct holder {\n             h http.header\n         }\n         fn main() {\n             var s holder\n             s.h.add(\"Content-Type\", \"application/json\")\n             s.h.set(\"Authorization\", \"Bearer token\")\n             _ = s.h.get(\"Authorization\")\n             _ = s.h.clone()\n             s.h.del(\"Authorization\")\n             fmt.println(1)\n         }\n",
    );
    assert_eq!(good.verdict, Verdict::Accept, "expected accept, got {:?}", good.diagnostics);
}

#[test]
fn real_http_transport_methods_accept() {
    let good = check(
        "import \"http\"\n         struct holder {\n             t http.transport\n         }\n         fn main() {\n             var s holder\n             s.t.close_idle_connections()\n             _ = s.t.clone()\n             fmt.println(1)\n         }\n",
    );
    assert_eq!(good.verdict, Verdict::Accept, "expected accept, got {:?}", good.diagnostics);
}
