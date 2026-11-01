use hy_check::{check_file, Verdict};

fn check(src: &str) -> hy_check::CheckResult {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn assert_rejects_with(src: &str, code: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Reject,
        "expected Reject with {code} for:\n{src}\ngot: {:?}",
        r.diagnostics
    );
    assert!(
        r.diagnostics.iter().any(|d| d.code == code),
        "expected diagnostic {code} for:\n{src}\ngot: {:?}",
        r.diagnostics
    );
}

fn assert_accepts(src: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected Accept for:\n{src}\ngot: {:?}",
        r.diagnostics
    );
}

#[test]
fn in_place_mutating_foreign_package_calls_on_const_roots_reject_e3006() {
    for src in [
        "struct stats { n int32 }\nfn main() {\n    const s = stats{n: 1}\n    atomic.store_int32(&s.n, 2)\n    _ = s\n}\n",
        "struct stats { n int64 }\nfn main() {\n    const s = stats{n: 1}\n    _ = atomic.add_int64(&s.n, 1)\n    _ = s\n}\n",
        "struct stats { n uint32 }\nfn main() {\n    const s = stats{n: 1}\n    _ = atomic.swap_uint32(&s.n, 2)\n    _ = s\n}\n",
        "struct stats { n int64 }\nfn main() {\n    const s = stats{n: 1}\n    _ = atomic.compare_and_swap_int64(&s.n, 1, 2)\n    _ = s\n}\n",
        "import \"rust/serde_json\"\nfn mutate_doc(d serde_json.value) error {\n    serde_json.set(d, \"k\", 1)?\n    return nil\n}\nfn main() error {\n    const doc = serde_json.new_object()\n    mutate_doc(doc)?\n    return nil\n}\n",
        "import \"encoding/json\"\nstruct cfg { port int }\nfn main() error {\n    const c = cfg{port: 80}\n    const raw = []byte{123, 125}\n    json.unmarshal(raw, &c)?\n    return nil\n}\n",
        "import \"encoding/hex\"\nfn main() error {\n    const dst = []byte{0, 0}\n    const src = []byte{48, 49}\n    _ = hex.decode(dst, src)?\n    return nil\n}\n",
    ] {
        assert_rejects_with(src, "HYB-E3006");
    }
}

#[test]
fn pinned_foreign_handle_surfaces_reject_phantom_methods_e1002() {
    for src in [
        "fn main() {\n    mux := http.new_serve_mux()\n    mux.totally_fake()\n}\n",
        "fn handle(w http.response_writer) {\n    w.totally_fake()\n}\nfn main() {}\n",
        "fn inspect(h http.header) {\n    h.totally_fake()\n}\nfn main() {}\n",
        "fn measure(d time.duration) {\n    d.totally_fake()\n}\nfn main() {}\n",
        "fn main() {\n    t0 := time.now()\n    dt := time.since(t0)\n    dt.totally_fake()\n}\n",
    ] {
        assert_rejects_with(src, "HYB-E1002");
    }
}

#[test]
fn multi_return_curated_stdlib_calls_enforce_tuple_arity_e2002() {
    for src in [
        "import \"encoding/json\"\nfn main() {\n    b := json.marshal(1)\n    _ = b\n}\n",
        "import \"encoding/json\"\nfn main() {\n    b := json.marshal_indent(1, \"\", \"  \")\n    _ = b\n}\n",
        "import \"net/url\"\nfn main() {\n    u := url.parse(\"http://example.com\")\n    _ = u\n}\n",
        "import \"net/url\"\nfn main() {\n    q := url.parse_query(\"a=1\")\n    _ = q\n}\n",
        "fn main() {\n    h := os.hostname()\n    _ = h\n}\n",
        "fn main() {\n    r, w := os.pipe()\n    _ = r\n    _ = w\n}\n",
        "import \"encoding/hex\"\nfn main() {\n    dst := []byte{0, 0}\n    src := []byte{48, 49}\n    n := hex.decode(dst, src)\n    _ = n\n}\n",
    ] {
        assert_rejects_with(src, "HYB-E2002");
    }
}

#[test]
fn void_atomic_store_result_is_not_sendable_primitive_e3005() {
    for src in [
        "fn w(x any) { _ = x }\nfn main() {\n    var n int32 = 1\n    task w(atomic.store_int32(&n, 2))\n}\n",
        "fn w(x any) { _ = x }\nfn main() {\n    task w(atomic.store_pointer(nil, nil))\n}\n",
    ] {
        assert_rejects_with(src, "HYB-E3005");
    }
}

#[test]
fn valid_foreign_methods_duration_chains_and_multi_return_bindings_accept() {
    assert_accepts(
        r#"import "encoding/json"
import "net/url"
fn handle(w http.response_writer) {
    hdr := w.header()
    hdr.set("X-Test", "ok")
}
fn main() error {
    t0 := time.now()
    dt := time.since(t0)
    ms := dt.milliseconds()
    b := json.marshal(ms)?
    u := url.parse("http://example.com")?
    r, w, err := os.pipe()
    _ = b
    _ = u
    _ = r
    _ = w
    _ = err
    return nil
}
"#,
    );
}
