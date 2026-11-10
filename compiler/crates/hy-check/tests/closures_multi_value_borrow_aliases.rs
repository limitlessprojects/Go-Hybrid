//! Regression tests for `closures.rs` recording borrow aliases (`st.borrow_bindings`
//! and `st.aliases` / `st.may_share`) on multi-value local declarations
//! (`r, n := borrow_pair(&x)`) inside closure bodies (`HYB-E3104`).

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let file = parse_file("test.hy", src).expect("parses");
    check_file(&file)
        .diagnostics
        .into_iter()
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn rejects_returning_multi_value_local_borrow_alias_inside_closure() {
    assert_eq!(
        codes(
            r#"import "fmt"
fn borrow_pair(p *int) (*int, int) {
    return p, 1
}
fn main() {
    f := fn() *int {
        x := 10
        r, n := borrow_pair(&x)
        _ = n
        return r
    }
    fmt.println(f())
}
"#,
        ),
        vec!["HYB-E3104"],
        "returning `r` from `r, n := borrow_pair(&x)` inside a closure must reject with HYB-E3104"
    );

    assert_eq!(
        codes(
            r#"import "fmt"
fn borrow_pair(p *int) (*int, int) {
    return p, 1
}
fn main() {
    f := fn() []*int {
        x := 10
        r, n := borrow_pair(&x)
        _ = n
        xs := []*int{r}
        return xs
    }
    fmt.println(len(f()))
}
"#,
        ),
        vec!["HYB-E3104"],
        "returning `xs := []*int{{r}}` carrying multi-value local borrow alias inside a closure must reject with HYB-E3104"
    );
}

#[test]
fn accepts_non_borrowed_multi_value_local_returns_inside_closure() {
    assert!(
        codes(
            r#"import "fmt"
fn fresh_pair() (*int, int) {
    return new(int), 1
}
fn main() {
    f := fn() *int {
        r, n := fresh_pair()
        _ = n
        return r
    }
    fmt.println(f())
}
"#,
        )
        .is_empty(),
        "returning a fresh `new(int)` pointer from a multi-value call inside a closure must pass with 0 diagnostics"
    );
}
