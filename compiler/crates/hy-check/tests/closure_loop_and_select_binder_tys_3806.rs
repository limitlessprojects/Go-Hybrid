use hy_check::check_file;
use hy_syntax::parse_file;

fn check_codes(src: &str) -> Vec<String> {
    check_file(&parse_file("test.hy", src).expect("parse"))
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn closure_for_and_select_binders_preserve_type_exprs() {
    // 1. Inside closures, ForKind::Range, ForKind::Iterate, and SelectArmKind::Recv
    // binders used as nested range bounds must reject out-of-range constants with HYB-E2009.
    for src in [
        "fn main() {\n  f := fn() {\n    ch := make(chan i8, 1)\n    select {\n      v, ok := <-ch: { if ok { for i in v..300 { _ = i } } }\n      default: {}\n    }\n  }\n  f()\n}",
        "fn main() {\n  f := fn() {\n    xs := []i16{1, 2}\n    for x in xs { for i in x..70000 { _ = i } }\n  }\n  f()\n}",
        "fn main() {\n  f := fn() {\n    m := map[i8]i16{1: 2}\n    for k, v in m {\n      for i in k..200 { _ = i }\n      _ = v\n    }\n  }\n  f()\n}",
        "fn main() {\n  f := fn() {\n    var n i8 = 10\n    for i in i8(0)..n { for j in i..300 { _ = j } }\n  }\n  f()\n}",
    ] {
        let codes = check_codes(src);
        assert!(
            codes.iter().any(|c| c == "HYB-E2009"),
            "expected HYB-E2009, got {codes:?} for:\n{src}"
        );
    }

    // 2. Inside closures, assignments, map-key indexing, and append on ForKind::Range / ForKind::Iterate
    // binders must reject overflowing constants with HYB-E6002.
    for src in [
        "fn main() {\n  f := fn() {\n    for i in i8(0)..i8(4) {\n      i = 300\n      _ = i\n    }\n  }\n  f()\n}",
        "fn main() {\n  f := fn() {\n    xs := []i8{1, 2}\n    for x in xs {\n      x = 300\n      _ = x\n    }\n  }\n  f()\n}",
        "fn main() {\n  f := fn() {\n    var ms []map[i8]int\n    for m in ms {\n      m[300] = 1\n    }\n  }\n  f()\n}",
        "fn main() {\n  f := fn() {\n    var rows [][]i8\n    for row in rows {\n      _ = append(row, 300)\n    }\n  }\n  f()\n}",
    ] {
        let codes = check_codes(src);
        assert!(
            codes.iter().any(|c| c == "HYB-E6002"),
            "expected HYB-E6002, got {codes:?} for:\n{src}"
        );
    }

    // 3. Positive controls: in-range bounds and assignments inside closures stay accepted (0 diagnostics).
    for src in [
        "fn main() {\n  f := fn() {\n    xs := []i8{1, 2}\n    for x in xs {\n      for i in x..100 { _ = i }\n      x = 120\n      _ = x\n    }\n    for j in i8(0)..i8(10) {\n      j = 50\n      _ = j\n    }\n  }\n  f()\n}",
    ] {
        let codes = check_codes(src);
        assert!(
            codes.is_empty(),
            "expected 0 diagnostics, got {codes:?} for:\n{src}"
        );
    }
}