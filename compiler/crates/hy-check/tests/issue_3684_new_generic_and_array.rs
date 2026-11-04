fn check(src: &str) -> Vec<hy_diag::Diagnostic> {
    hy_check::check_file(&hy_syntax::parse_file("t.hy", src).unwrap()).diagnostics
}

#[test]
fn test_new_generic_accepted() {
    let src = "struct box[t] {\n    v t\n}\nfn main() {\n    p := new(box[int])\n    _ = p\n}\n";
    let diags = check(src);
    assert!(
        diags.is_empty(),
        "expected zero diagnostics for valid new(G[T]), got: {:?}",
        diags.iter().map(|d| (&d.code, &d.message)).collect::<Vec<_>>()
    );
}

#[test]
fn test_new_generic_type_instantiation_validation() {
    // 1. Non-generic struct instantiated with type args: HYB-E1002
    let src1 = "struct plain {\n    v int\n}\nfn main() {\n    _ = new(plain[int])\n}\n";
    let diags1 = check(src1);
    assert!(
        diags1.iter().any(|d| d.code == "HYB-E1002"),
        "expected HYB-E1002 for new(plain[int]), got: {:?}",
        diags1.iter().map(|d| (&d.code, &d.message)).collect::<Vec<_>>()
    );

    // 2. Incomparable set element type: HYB-E1002
    let src2 = "fn main() {\n    _ = new(set[[]int])\n}\n";
    let diags2 = check(src2);
    assert!(
        diags2.iter().any(|d| d.code == "HYB-E1002"),
        "expected HYB-E1002 for new(set[[]int]), got: {:?}",
        diags2.iter().map(|d| (&d.code, &d.message)).collect::<Vec<_>>()
    );

    // 3. Indexing on local variable: HYB-E2005
    let src3 = "fn main() {\n    var arr = [2]int{1, 2}\n    _ = new(arr[0])\n}\n";
    let diags3 = check(src3);
    assert!(
        diags3.iter().any(|d| d.code == "HYB-E2005"),
        "expected HYB-E2005 for new(arr[0]), got: {:?}",
        diags3.iter().map(|d| (&d.code, &d.message)).collect::<Vec<_>>()
    );
}

#[test]
fn test_new_array_pointer_operations() {
    let src = "fn main() {\n    p := new([2]int{})\n    _ = len(p)\n    _ = cap(p)\n    _ = p[0]\n    _ = p[0:1]\n}\n";
    let diags = check(src);
    assert!(
        diags.is_empty(),
        "expected zero diagnostics for array pointer operations, got: {:?}",
        diags.iter().map(|d| (&d.code, &d.message)).collect::<Vec<_>>()
    );
}
