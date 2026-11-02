fn codes(src: &str) -> Vec<String> {
    let f = hy_syntax::parse_file("test.hy", src).expect("parse");
    hy_check::check_file(&f).diagnostics.into_iter().map(|d| d.code).collect()
}

#[test]
fn rejects_nil_in_len_cap_delete_min_max() {
    for src in [
        "fn main() { _ = len(nil) }",
        "fn main() { _ = cap(nil) }",
        "fn main() { delete(nil, 1) }",
        "fn main() { delete(42, 1) }",
        "fn main() { _ = min(1, nil) }",
        "fn main() { _ = max(nil, 2) }",
    ] {
        assert!(codes(src).iter().any(|c| c == "HYB-E2005"), "expected HYB-E2005 for: {src}");
    }
}
