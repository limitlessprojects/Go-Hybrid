// Issue #4695: Verify that user declarations or local uses of prelude
    // helper names like `hy_pop` and `hy_n5_make` do not self-trigger
    // injections or suffer from capture collisions.
    let code = r#"
        fn hy_pop(xs: [int]) -> int {
            return 0;
        }

        fn main() {
            let hy_n5_make = 42;
            let mut list = [1, 2, 3];
            let val = list.pop();
            print(val);
            print(hy_n5_make);
        }
    "";
    
    // Check that hy-check compiles and validates this correctly without
    // unguarded helper collision or unsound injection.
    let file = crate::parse_str(code).expect("test source should parse");
    let result = crate::check_file(&file);
    assert!(
        result.diagnostics.is_empty(),
        "prelude helper names should be properly guarded, got diagnostics: {:?}",
        result.diagnostics
    );
}