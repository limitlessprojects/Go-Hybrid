//! Regression tests for issue #2585:
//! HYB-E3013: check bypassed when the closure is passed via a variable or slice,
//! and `managed("...", "interop")` does not suppress it.

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

fn assert_rejected(src: &str, want: &str) {
    let cs = codes(src);
    assert!(
        cs.iter().any(|c| c == want),
        "expected [{want}] among diagnostics, got {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(cs.is_empty(), "expected 0 diagnostics, got {cs:?} for:\n{src}");
}

#[test]
fn closure_via_variable_into_foreign_call_is_rejected() {
    let src = "import \"net/http\"\n\
               fn main() {\n\
                   hits := 0\n\
                   h := fn(w, r) {\n\
                       hits = hits + 1\n\
                   }\n\
                   mux := http.new_serve_mux()\n\
                   mux.handle(\"/x\", http.handler(h))\n\
               }\n";
    assert_rejected(src, "HYB-E3013");
}

#[test]
fn closure_via_slice_into_foreign_call_is_rejected() {
    let src = "import \"net/http\"\n\
               fn main() {\n\
                   hits := 0\n\
                   handlers := [fn(w, r) {\n\
                       hits = hits + 1\n\
                   }]\n\
                   mux := http.new_serve_mux()\n\
                   mux.handle(\"/x\", http.handler(handlers[0]))\n\
               }\n";
    assert_rejected(src, "HYB-E3013");
}

#[test]
fn closure_inline_into_foreign_call_is_rejected() {
    let src = "import \"net/http\"\n\
               fn main() {\n\
                   hits := 0\n\
                   mux := http.new_serve_mux()\n\
                   mux.handle(\"/x\", http.handler(fn(w, r) {\n\
                       hits = hits + 1\n\
                   }))\n\
               }\n";
    assert_rejected(src, "HYB-E3013");
}

#[test]
fn managed_interop_suppresses_e3013_inline() {
    let src = "import \"net/http\"\n\
               fn main() {\n\
                   hits := 0\n\
                   mux := http.new_serve_mux()\n\
                   managed(\"shared counter\", \"interop\") {\n\
                       mux.handle(\"/x\", http.handler(fn(w, r) {\n\
                           hits = hits + 1\n\
                       }))\n\
                   }\n\
               }\n";
    assert_ok(src);
}

#[test]
fn managed_interop_suppresses_e3013_via_variable() {
    let src = "import \"net/http\"\n\
               fn main() {\n\
                   hits := 0\n\
                   h := fn(w, r) {\n\
                       hits = hits + 1\n\
                   }\n\
                   mux := http.new_serve_mux()\n\
                   managed(\"shared counter\", \"interop\") {\n\
                       mux.handle(\"/x\", http.handler(h))\n\
                   }\n\
               }\n";
    assert_ok(src);
}

#[test]
fn managed_interop_suppresses_e3013_via_slice() {
    let src = "import \"net/http\"\n\
               fn main() {\n\
                   hits := 0\n\
                   handlers := [fn(w, r) {\n\
                       hits = hits + 1\n\
                   }]\n\
                   mux := http.new_serve_mux()\n\
                   managed(\"shared counter\", \"interop\") {\n\
                       mux.handle(\"/x\", http.handler(handlers[0]))\n\
                   }\n\
               }\n";
    assert_ok(src);
}

#[test]
fn closure_via_variable_read_only_is_accepted() {
    let src = "import \"net/http\"\n\
               fn main() {\n\
                   hits := 0\n\
                   h := fn(w, r) {\n\
                       _ = hits\n\
                   }\n\
                   mux := http.new_serve_mux()\n\
                   mux.handle(\"/x\", http.handler(h))\n\
               }\n";
    assert_ok(src);
}
