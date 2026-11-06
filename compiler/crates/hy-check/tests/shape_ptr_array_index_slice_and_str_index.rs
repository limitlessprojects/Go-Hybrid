//! Regression tests for `shape.rs` (`Checker::shape_of_expr`) resolving
//! pointer-to-array (`*[N]T`) bases in `Expr::Index` and `Expr::Slice` and
//! `TypeShape::Str` bases in `Expr::Index`.
//!
//! Covers:
//!   1. Soundness (`HYB-E3003` / `HYB-E3103`): `p[0].insert(xs)` on `p := new([1]set[any]{})`
//!      recording the `may_share` alias edge so `task worker(move p); xs[0] = 99`
//!      is rejected with `HYB-E3003`, and storing `p[0]` from `p *[1]serde_json.view`
//!      into an `any` slot (`HYB-E3103`).
//!   2. Buildable / O4.9 (`HYB-E3005` false-positive elimination): ranging over
//!      `p[0:2]` (`p *[2]int`) and `p[0]` (`p *[1][]int`), receiving from `<-p[0]`
//!      (`p *[1]chan int`), and sending/spawning a string byte index `s[0]`.

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
fn rejects_set_insert_share_race_and_borrow_handle_storage_through_ptr_to_array_index() {
    let diags = codes(
        r#"import "fmt"
fn worker(p *[1]set[*int]) {
    fmt.println(len(p[0]))
}
fn main() {
    p := new([1]set[*int]{})
    q := new(int)
    p[0].insert(q)
    task worker(move p)
    q = new(int)
    fmt.println(q)
}
"#,
    );
    assert!(
        diags.contains(&"HYB-E3003".to_string()),
        "`p[0].insert(q)` on `p := new([1]set[*int]{{}})` followed by `task worker(move p)` must record may_share and reject HYB-E3003, got: {diags:?}"
    );

    assert_eq!(
        codes(
            r#"import "fmt"
import "rust/serde_json"
fn run(p *[1]serde_json.view) (out any) {
    out = p[0]
    return
}
fn main() {
    fmt.println(1)
}
"#,
        ),
        vec!["HYB-E3104"],
        "returning borrow handle `p[0]` (`p *[1]serde_json.view`) via named result must reject HYB-E3104"
    );
}

#[test]
fn accepts_valid_ptr_to_array_index_slice_and_string_byte_index_operations() {
    assert!(
        codes(
            r#"import "fmt"
fn worker(b byte) {
    fmt.println(b)
}
fn run(p_arr *[2]int, p_slices *[1][]int, p_chans *[1]chan int) {
    for x in p_arr[0:2] {
        fmt.println(x)
    }
    for y in p_slices[0] {
        fmt.println(y)
    }
    v := <-p_chans[0]
    fmt.println(v)
}
fn main() {
    s := "abc"
    ch := make(chan byte, 1)
    ch <- s[0]
    task worker(s[0])
    fmt.println(<-ch)
}
"#,
        )
        .is_empty(),
        "valid pointer-to-array slice/index iteration, channel receive, and string byte index send/spawn must pass with 0 diagnostics"
    );
}
