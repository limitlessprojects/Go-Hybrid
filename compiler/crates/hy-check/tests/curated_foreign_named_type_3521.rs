use hy_check::{Verdict, check_file};
use hy_syntax::parse_file;

#[test]
fn curated_package_types_classify_as_foreign_opaque_at_move_boundaries_3521() {
    for src in [
        "import \"bufio\"\nstruct s { r bufio.reader }\nfn w(x s) { _ = x }\nfn main() { j := s{}\n task w(move j) }",
        "import \"net/url\"\nstruct s { u url.url }\nfn w(x s) { _ = x }\nfn main() { j := s{}\n task w(move j) }",
        "import \"encoding/json\"\nfn w(d json.decoder) { _ = d }\nfn main() { var d json.decoder\n task w(move d) }",
    ] {
        let r = check_file(&parse_file("t.hy", src).unwrap());
        assert!(r.verdict == Verdict::Reject && r.diagnostics.iter().any(|d| d.code == "HYB-E3001"), "{src}: {:?}", r.diagnostics);
    }
}