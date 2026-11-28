use hy_check::check_file;
use hy_syntax::parse_file;

fn check_codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("test source must parse");
    let report = check_file(&ast);
    report.diagnostics.into_iter().map(|d| d.code).collect()
}

#[test]
fn let_slice_pop_rejects_with_e3006() {
    let src = "\
fn main() {
    let s = []int{1, 2}
    _ = s.pop()
}
";
    let codes = check_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 on `let s = ...; s.pop()`, got {:?}",
        codes
    );
}

#[test]
fn parenthesized_let_slice_pop_rejects_with_e3006() {
    let src = "\
fn main() {
    let s = []int{1, 2}
    _ = (s).pop()
}
";
    let codes = check_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 on `let s = ...; (s).pop()`, got {:?}",
        codes
    );
}

#[test]
fn closure_pop_on_outer_let_slice_rejects_with_e3006() {
    let src = "\
fn main() {
    let s = []int{1, 2}
    f := fn() {
        _ = s.pop()
    }
    f()
}
";
    let codes = check_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 on closure `s.pop()` of outer `let` slice, got {:?}",
        codes
    );
}

#[test]
fn mutable_slice_and_let_struct_method_pop_and_element_write_stay_accepted() {
    let mutable_src = "\
fn main() {
    s := []int{1, 2}
    _ = s.pop()
}
";
    assert!(
        check_codes(mutable_src).is_empty(),
        "mutable slice `.pop()` must stay accepted"
    );

    let struct_method_src = "\
struct holder { n int }
fn (h *holder) pop() int {
    return h.n
}
fn main() {
    let h = holder{n: 1}
    _ = h.pop()
}
";
    assert!(
        check_codes(struct_method_src).is_empty(),
        "user struct method `pop()` on `let` binding must stay accepted"
    );

    let elem_write_src = "\
fn main() {
    let s = []int{1, 2}
    s[0] = 9
    _ = s
}
";
    assert!(
        check_codes(elem_write_src).is_empty(),
        "interior element write on `let` slice must stay accepted"
    );

    let shadowed_src = "\
fn main() {
    let s = []int{1, 2}
    if true {
        s := []int{3, 4}
        _ = s.pop()
    }
    _ = s
}
";
    assert!(
        check_codes(shadowed_src).is_empty(),
        "inner mutable `s := ...` shadowing outer `let s` must stay accepted"
    );
}
