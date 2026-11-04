use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let f = parse_file("test.hy", src).expect("parses");
    check_file(&f).diagnostics.into_iter().map(|d| d.code).collect()
}

#[test]
fn nested_array_constant_index_bounds_issue_3694() {
    // Row 1: nested 2D array out of bounds
    let r1 = "import \"fmt\"\nfn main() {\n    var a [2][3]int\n    fmt.println(a[1][3])\n}\n";
    assert!(codes(r1).iter().any(|c| c == "HYB-E1002"), "Row 1 should reject: {:?}", codes(r1));

    // Row 2: nested 3D array out of bounds
    let r2 = "import \"fmt\"\nfn main() {\n    var a [2][3][4]int\n    fmt.println(a[1][2][4])\n}\n";
    assert!(codes(r2).iter().any(|c| c == "HYB-E1002"), "Row 2 should reject: {:?}", codes(r2));

    // Row 3: nested array write index out of bounds
    let r3 = "fn main() {\n    var a [2][3]int\n    a[1][3] = 1\n}\n";
    assert!(codes(r3).iter().any(|c| c == "HYB-E1002"), "Row 3 should reject: {:?}", codes(r3));

    // Row 4: struct field array index out of bounds
    let r4 = "struct s_t {\n    a [3]int\n}\nfn main() {\n    var s s_t\n    _ = s.a[3]\n}\n";
    assert!(codes(r4).iter().any(|c| c == "HYB-E1002"), "Row 4 should reject: {:?}", codes(r4));

    // Row 5: array of structs field array index out of bounds
    let r5 = "struct s_t {\n    a [3]int\n}\nfn main() {\n    var s [2]s_t\n    _ = s[0].a[3]\n}\n";
    assert!(codes(r5).iter().any(|c| c == "HYB-E1002"), "Row 5 should reject: {:?}", codes(r5));

    // Row 6: call result array index out of bounds
    let r6 = "fn f() [3]int {\n    var a [3]int\n    return a\n}\nfn main() {\n    _ = f()[3]\n}\n";
    assert!(codes(r6).iter().any(|c| c == "HYB-E1002"), "Row 6 should reject: {:?}", codes(r6));

    // Row 7: slice of arrays index out of bounds
    let r7 = "fn main() {\n    var s [][3]int\n    _ = s[0][3]\n}\n";
    assert!(codes(r7).iter().any(|c| c == "HYB-E1002"), "Row 7 should reject: {:?}", codes(r7));

    // Row 8: pointer to array index out of bounds
    let r8 = "fn main() {\n    var a [2][3]int\n    p := &a\n    _ = p[1][3]\n}\n";
    assert!(codes(r8).iter().any(|c| c == "HYB-E1002"), "Row 8 should reject: {:?}", codes(r8));

    // Row 9: nested slice bound out of bounds
    let r9 = "import \"fmt\"\nfn main() {\n    var a [2][3]int\n    fmt.println(a[1][0:5])\n}\n";
    assert!(codes(r9).iter().any(|c| c == "HYB-E1002"), "Row 9 should reject: {:?}", codes(r9));

    // Controls: within bounds should pass
    let ok = "import \"fmt\"\nstruct s_t {\n    a [3]int\n}\nfn f() [3]int {\n    var a [3]int\n    return a\n}\nfn main() {\n    var a [2][3]int\n    a[1][2] = 1\n    var s s_t\n    var arr [2]s_t\n    p := &a\n    fmt.println(a[1][2], s.a[2], arr[0].a[2], f()[2], p[1][2], a[1][0:2])\n}\n";
    assert!(codes(ok).is_empty(), "In-bounds controls should pass: {:?}", codes(ok));
}
