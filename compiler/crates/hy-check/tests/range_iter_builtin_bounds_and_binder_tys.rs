use hy_check::check_file;
use hy_syntax::parse_file;

fn check_codes(src: &str) -> Vec<String> {
    check_file(&parse_file("test.hy", src).expect("parse")).diagnostics.into_iter().map(|d| d.code).collect()
}

#[test]
fn range_bound_builtins_and_binder_type_exprs() {
    for src in [
        "fn main() {\n xs := []int{1, 2, 3}\n var lo i32 = 0\n for i in lo..len(xs) { _ = i }\n}",
        "fn main() {\n xs := []int{1, 2, 3}\n var lo i16 = 0\n for i in lo..cap(xs) { _ = i }\n}",
        "fn main() {\n a := []int{1, 2}\n b := []int{3, 4}\n var lo i64 = 0\n for i in lo..copy(a, b) { _ = i }\n}",
        "fn main() {\n var a i8 = 3\n var b i8 = 5\n for i in 0..min(a, b) { _ = i }\n}",
        "fn main() {\n var a i8 = 3\n var b i8 = 5\n for i in min(a, b)..300 { _ = i }\n}",
        "fn main() {\n var lo i32 = 0\n for i in lo..max(1, 2) { _ = i }\n}",
        "fn main() {\n ch := make(chan i8, 1)\n select {\n v, ok := <-ch: { if ok { for i in v..300 { _ = i } } }\n default: {}\n }\n}",
        "fn main() {\n xs := []i16{1, 2}\n for x in xs { for i in x..70000 { _ = i } }\n}",
        "fn main() {\n m := map[i8]i16{1: 2}\n for k, v in m {\n for i in k..200 { _ = i }\n _ = v\n }\n}",
        "fn main() {\n var n i8 = 10\n for i in i8(0)..n { for j in i..300 { _ = j } }\n}",
    ] {
        assert!(check_codes(src).iter().any(|c| c == "HYB-E2009"), "expected HYB-E2009:\n{src}");
    }
    for src in [
        "fn main() {\n xs := []int{1, 2, 3}\n for i in 0..len(xs) { _ = i }\n}",
        "fn main() {\n var a i8 = 1\n var b i8 = 5\n for i in min(a, b)..10 { _ = i }\n for j in 0..min(1, 2) { _ = j }\n}",
    ] {
        assert!(check_codes(src).is_empty(), "expected 0 diagnostics:\n{src}");
    }
}
