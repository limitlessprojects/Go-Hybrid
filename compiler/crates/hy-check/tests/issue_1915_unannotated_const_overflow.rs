//! #1915 — unannotated file-level `const` whose initializer carries a fixed-width
//! integer type (`const top = u8(255)` or `const half = u8(1) << 7`) propagates
//! that type into constant expressions, while locally shadowed names do not.

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

#[test]
fn unannotated_file_const_with_typed_initializer_rejects_overflow() {
    let c1 = codes("const top = u8(255)\n\nfn main() {\n    a := top + 1\n    fmt.println(\"{a}\")\n}\n");
    assert!(c1.iter().any(|c| c == "HYB-E6002"), "expected HYB-E6002, got {c1:?}");

    let c2 = codes("const half = u8(1) << 7\n\nfn main() {\n    a := half + half\n    fmt.println(\"{a}\")\n}\n");
    assert!(c2.iter().any(|c| c == "HYB-E6002"), "expected HYB-E6002, got {c2:?}");
}

#[test]
fn locally_shadowed_const_does_not_falsely_trigger_overflow() {
    let c = codes("const top u8 = 255\n\nfn main() {\n    var top int = 255\n    a := top + 1\n    fmt.println(\"{a}\")\n}\n");
    assert!(!c.iter().any(|c| c == "HYB-E6002"), "unexpected HYB-E6002: {c:?}");
}
