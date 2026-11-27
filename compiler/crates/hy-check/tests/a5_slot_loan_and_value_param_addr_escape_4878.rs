//! Regression tests for A5 Go-escape check-site (`HYB-E3013`, #4878).
use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("a5_slot_loan_escape.hy", src).expect("snippet must parse");
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<&str> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.as_str())
        .collect()
}

fn assert_rejects_e3013(src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject for:\n{src}\ngot: {:?}", r.diagnostics);
    assert_eq!(error_codes(&r), vec!["HYB-E3013"], "expected exact [HYB-E3013] for:\n{src}\ngot: {:?}", r.diagnostics);
}

fn assert_accepts_clean(src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept for:\n{src}\ngot: {:?}", r.diagnostics);
    assert!(error_codes(&r).is_empty(), "expected 0 errors for:\n{src}\ngot: {:?}", r.diagnostics);
}

#[test]
fn o8_slot_loan_unconditional_store_backs_reject_e3013() {
    for src in [
        "import \"net/http\"\nfn main() {\n  m := map[string]int{\"k\": 1}\n  mux := http.new_serve_mux()\n  mux.handle(\"/x\", http.handler(fn() {\n    p := &m[\"k\"]\n    _ = p\n  }))\n}",
        "import \"net/http\"\nstruct item { v int }\nfn main() {\n  m := map[string]item{\"k\": item{v: 1}}\n  http.handle_func(\"/x\", fn(w http.response_writer, r *http.request) {\n    p := &m[\"k\"]\n    _ = p.v\n  })\n}",
        "import \"net/http\"\nstruct item { v int }\nfn main() {\n  m := map[string]item{\"k\": item{v: 1}}\n  http.handle_func(\"/x\", fn(w http.response_writer, r *http.request) {\n    local_item := item{v: 0}\n    p := &local_item\n    p = &m[\"k\"]\n    _ = p.v\n  })\n}",
        "import \"net/http\"\nstruct sub { v int }\nstruct outer { sub sub }\nfn main() {\n  s := outer{sub: sub{v: 1}}\n  http.handle_func(\"/x\", fn(w http.response_writer, r *http.request) {\n    p := &s.sub\n    _ = p.v\n  })\n  _ = s\n}",
        "import \"net/http\"\nstruct item { v int }\nfn main() {\n  xs := []item{item{v: 1}}\n  http.handle_func(\"/x\", fn(w http.response_writer, r *http.request) {\n    p := &xs[0]\n    _ = p.v\n  })\n}",
        "import \"net/http\"\nfn read_map_slot(m map[string]int) {\n  p := &m[\"k\"]\n  _ = p\n}\nfn main() {\n  m := map[string]int{\"k\": 1}\n  http.handle_func(\"/x\", fn(w http.response_writer, r *http.request) {\n    read_map_slot(m)\n  })\n}",
        "import \"net/http\"\nstruct holder { m map[string]int }\nfn (h holder) read_slot(w http.response_writer, r *http.request) {\n  p := &h.m[\"k\"]\n  _ = p\n}\nfn main() {\n  h := holder{m: map[string]int{\"k\": 1}}\n  http.handle_func(\"/x\", h.read_slot)\n  _ = h\n}",
    ] {
        assert_rejects_e3013(src);
    }
}

#[test]
fn value_passed_composite_addr_and_ptr_receiver_interior_mutations_reject_e3013() {
    for src in [
        "import \"net/http\"\nstruct holder { m map[string]int\n n int }\nfn sneak_addr(h holder) {\n  p := &h\n  p.m[\"k\"] = 99\n}\nfn main() {\n  h := holder{m: map[string]int{\"k\": 1}, n: 0}\n  http.handle_func(\"/x\", fn(w http.response_writer, r *http.request) { sneak_addr(h) })\n  _ = h\n}",
        "import \"net/http\"\nstruct holder { m map[string]int }\nfn sneak_del(h holder) {\n  p := &h\n  delete(p.m, \"k\")\n}\nfn main() {\n  h := holder{m: map[string]int{\"k\": 1}}\n  http.handle_func(\"/x\", fn(w http.response_writer, r *http.request) { sneak_del(h) })\n  _ = h\n}",
        "import \"net/http\"\nstruct holder { m map[string]int }\nfn write_ptr(p *holder) { p.m[\"k\"] = 99 }\nfn sneak_ptr_call(h holder) { write_ptr(&h) }\nfn main() {\n  h := holder{m: map[string]int{\"k\": 1}}\n  http.handle_func(\"/x\", fn(w http.response_writer, r *http.request) { sneak_ptr_call(h) })\n  _ = h\n}",
        "import \"net/http\"\nstruct holder { m map[string]int }\nfn (h *holder) stomp_ptr() { h.m[\"k\"] = 99 }\nfn sneak_ptr_recv(h holder) { h.stomp_ptr() }\nfn main() {\n  h := holder{m: map[string]int{\"k\": 1}}\n  http.handle_func(\"/x\", fn(w http.response_writer, r *http.request) { sneak_ptr_recv(h) })\n  _ = h\n}",
    ] {
        assert_rejects_e3013(src);
    }
}

#[test]
fn closure_local_slot_loans_and_value_copy_scalar_mutations_stay_accepted() {
    for src in [
        "import \"net/http\"\nstruct item { v int }\nfn main() {\n  http.handle_func(\"/x\", fn(w http.response_writer, r *http.request) {\n    m := map[string]item{\"k\": item{v: 1}}\n    p := &m[\"k\"]\n    p.v = 2\n  })\n}",
        "import \"net/http\"\nstruct stats { n int }\nfn main() {\n  s := stats{n: 1}\n  http.handle_func(\"/x\", fn(w http.response_writer, r *http.request) {\n    var p *stats = &s\n    _ = p.n\n  })\n}",
        "import \"net/http\"\nstruct sub { v int }\nstruct mixed {\n  m map[string]int\n  sub sub\n  n int\n  xs [2]int\n}\nfn touch_ptr(p *mixed) {\n  p.n = 7\n  p.xs[0] = 8\n}\nfn (x *mixed) touch_recv() {\n  x.n = 9\n  x.xs[1] = 10\n}\nfn (x mixed) touch_copy_all(w http.response_writer, r *http.request) {\n  slot := &x.sub\n  slot.v = 1\n  p := &x\n  p.n = 2\n  p.xs[0] = 3\n  touch_ptr(&x)\n  x.touch_recv()\n}\nfn main() {\n  mx := mixed{m: map[string]int{\"k\": 1}, sub: sub{v: 0}, n: 0, xs: [2]int{0, 0}}\n  http.handle_func(\"/x\", mx.touch_copy_all)\n  _ = mx\n}",
        "import \"net/http\"\nfn main() {\n  m := map[string]int{\"k\": 1}\n  managed(\"explicit handler lock\", \"interop\", \"#1\") {\n    http.handle_func(\"/x\", fn(w http.response_writer, r *http.request) {\n      p := &m[\"k\"]\n      _ = p\n    })\n  }\n}",
    ] {
        assert_accepts_clean(src);
    }
}
