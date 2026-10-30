use hy_syntax::{parse_file, Expr, Stmt};

#[test]
fn full_slice_records_max_and_optional_low() {
    for (expression, has_lo) in [("s[1:2:4]", true), ("s[:2:4]", false)] {
        let file = parse_file("slice.hy", &format!("fn f() {{ _ = {expression} }}")).unwrap();
        let Stmt::Assign { values, .. } = &file.fns().next().unwrap().body.as_ref().unwrap().stmts[0] else { panic!() };
        let Expr::Slice { lo, hi, max, .. } = &values[0] else { panic!() };
        assert_eq!(lo.is_some(), has_lo);
        assert_eq!(hi.as_deref().unwrap().render(), "2");
        assert_eq!(max.as_deref().unwrap().render(), "4");
    }
}

#[test]
fn incomplete_or_extra_full_slice_bounds_are_parse_errors() {
    for expression in ["s[::4]", "s[1::4]", "s[1:2:]", "s[:2:]", "s[1:2:3:4]"] {
        assert!(parse_file("bad.hy", &format!("fn f() {{ _ = {expression} }}")).is_err(), "{expression}");
    }
}
