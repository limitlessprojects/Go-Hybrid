use hy_check::check_file;
use hy_syntax::parse_file;

fn codes_of(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("parse");
    let res = check_file(&ast);
    res.diagnostics.into_iter().map(|d| d.code).collect()
}

#[test]
fn assign_file_level_const_ident_and_expr_overflows_reject_e6002() {
    let src = "const big = 300\n\
               const part = 200\n\
               fn main() {\n\
               \tvar x i8 = 1\n\
               \tx = big\n\
               \tvar y i8 = 1\n\
               \ty = part + 100\n\
               \tvar z i8 = 1\n\
               \tz += big\n\
               \t_ = x\n\
               \t_ = y\n\
               \t_ = z\n\
               }\n";
    let codes = codes_of(src);
    let e6002_count = codes.iter().filter(|c| c.as_str() == "HYB-E6002").count();
    assert_eq!(
        e6002_count, 3,
        "expected 3 HYB-E6002 diagnostics on const assignment overflows, got: {:?}",
        codes
    );
}

#[test]
fn float_const_ident_and_folded_expr_assigned_to_integer_types_reject_e6002() {
    let src = "const f = 2.5\n\
               const big_f = 300.0\n\
               fn main() {\n\
               \tvar a int = f\n\
               \tvar b int = 1.5 + 1.0\n\
               \tvar c i8 = big_f\n\
               \tvar d i8 = 150.0 + 150.0\n\
               \txs := []int{f, 1.5 + 1.0}\n\
               \tvar e i8 = 1\n\
               \te = 1.5 + 1.0\n\
               \tvar g i8 = 1\n\
               \tg = 150.0 + 150.0\n\
               \t_ = a\n\
               \t_ = b\n\
               \t_ = c\n\
               \t_ = d\n\
               \t_ = xs\n\
               \t_ = e\n\
               \t_ = g\n\
               }\n";
    let codes = codes_of(src);
    let e6002_count = codes.iter().filter(|c| c.as_str() == "HYB-E6002").count();
    assert_eq!(
        e6002_count, 8,
        "expected 8 HYB-E6002 diagnostics on float const ident/expr to int, got: {:?}",
        codes
    );
}

#[test]
fn block_scoped_local_consts_overflow_in_init_assign_default_int_and_composites() {
    let src = "struct box_i8 {\n\
               \tv i8\n\
               }\n\
               fn take_i8(v i8) {\n\
               \t_ = v\n\
               }\n\
               fn main() {\n\
               \tconst local_big = 300\n\
               \tconst local_f = 2.5\n\
               \tconst local_huge = 1 << 70\n\
               \tconst local_inf = 1e300 * 1e100\n\
               \tvar a i8 = local_big\n\
               \tvar b i8 = 1\n\
               \tb = local_big\n\
               \tvar c int = local_f\n\
               \td := local_huge\n\
               \te := local_inf\n\
               \ts := box_i8{v: local_big}\n\
               \txs := []i8{local_big}\n\
               \tm := map[i8]int{local_big: 1}\n\
               \ttake_i8(local_big)\n\
               \t_ = a\n\
               \t_ = b\n\
               \t_ = c\n\
               \t_ = d\n\
               \t_ = e\n\
               \t_ = s\n\
               \t_ = xs\n\
               \t_ = m\n\
               }\n";
    let codes = codes_of(src);
    let e6002_count = codes.iter().filter(|c| c.as_str() == "HYB-E6002").count();
    assert_eq!(
        e6002_count, 9,
        "expected 9 HYB-E6002 diagnostics on block-scoped local const overflows, got: {:?}",
        codes
    );
}

#[test]
fn runtime_locals_shadowing_consts_and_in_range_consts_accept_cleanly() {
    let src = "const big = 300\n\
               const f_ok = 2.0\n\
               struct box_i8 {\n\
               \tv i8\n\
               }\n\
               fn take_i8(v i8) {\n\
               \t_ = v\n\
               }\n\
               fn main() {\n\
               \tconst local_ok = 42\n\
               \tvar a i8 = local_ok\n\
               \tvar b i8 = f_ok\n\
               \tbig := i8(7)\n\
               \tvar c i8 = 1\n\
               \tc = big\n\
               \ts := box_i8{v: big}\n\
               \txs := []i8{big}\n\
               \tm := map[i8]int{big: 1}\n\
               \ttake_i8(big)\n\
               \t_ = a\n\
               \t_ = b\n\
               \t_ = c\n\
               \t_ = s\n\
               \t_ = xs\n\
               \t_ = m\n\
               }\n";
    let codes = codes_of(src);
    assert!(
        codes.is_empty(),
        "expected 0 diagnostics when runtime locals shadow consts or consts fit range, got: {:?}",
        codes
    );
}
