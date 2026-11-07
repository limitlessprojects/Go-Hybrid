//! Regression tests for multi-return foreign handle unpacking and curated stdlib shapes.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn passes(src: &str) {
    let r = check_file(&parse_file("inline.hy", src).expect("parse"));
    assert_eq!(r.verdict, Verdict::Accept, "expected Accept, got: {:?}", r.diagnostics);
}

fn rejects_with(src: &str, code: &str) {
    let r = check_file(&parse_file("inline.hy", src).expect("parse"));
    assert!(r.verdict == Verdict::Reject && r.diagnostics.iter().any(|d| d.severity == Severity::Error && d.code == code), "expected {code}, got: {:?}", r.diagnostics);
}

#[test]
fn multireturn_foreign_handle_unpack_tracks_borrow_loan_and_post_move() {
    rejects_with("fn main() {\ndoc, err := serde_json.parse(\"[]\")\n_ = err\nitems, err2 := doc.array(\"k\")\n_ = err2\ndoc.set(\"x\", \"y\")\n_ = items.len()\n}", "HYB-E3101");
    rejects_with("fn consume(d serde_json.value) { _ = d }\nfn main() {\ndoc, err := serde_json.parse(\"[]\")\n_ = err\ntask consume(move doc)\n_ = doc.string(\"k\")\n}", "HYB-E3003");
}

#[test]
fn parenthesized_package_root_preserves_handle_shape() {
    rejects_with("fn main() error {\ndoc := (serde_json).parse(\"[]\")?\nitems := doc.array(\"k\")?\ndoc.set(\"x\", \"y\")\n_ = items.len()\nreturn nil\n}", "HYB-E3101");
}

#[test]
fn curated_packages_recognized_without_explicit_import_and_shapes_enforced() {
    passes("fn main() error {\ns := url.query_escape(\"a b\")\nh := hex.encode_to_string(s)\np := path.join(\"a\", h)\nfp := filepath.clean(p)\nok := json.valid(fp)\n_ = ok\nreturn nil\n}");
    rejects_with("fn worker(q url.values) { _ = q }\nfn main() error {\nq := url.parse_query(\"a=1\")?\ntask worker(q)\nreturn nil\n}", "HYB-E3001");
    rejects_with("fn worker(b []byte) { _ = b }\nfn main() error {\nraw, err := json.marshal(\"hi\")\n_ = err\ntask worker(raw)\nreturn nil\n}", "HYB-E3001");
}

#[test]
fn struct_wrapping_foreign_opaque_or_unproven_atomic_counts_as_sharing_in_freeze() {
    rejects_with("struct holder { mux http.serve_mux }\nfn main() {\nh := holder{mux: http.new_serve_mux()}\nalias := h\nconst shared = move h\n_ = alias\n_ = shared\n}", "HYB-E3003");
    rejects_with("struct box_val { v atomic.value }\nfn main() {\nb := box_val{}\nalias := b\nconst shared = move b\n_ = alias\n_ = shared\n}", "HYB-E3003");
}
