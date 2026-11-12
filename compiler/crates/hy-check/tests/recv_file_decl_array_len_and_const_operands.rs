//! Regression tests for `recv.rs` (`check_file_decl_recv_operands`) enforcing
//! `HYB-E2008` / `HYB-E3005` on receive operands inside file-level `const`,
//! `struct`, `type` alias, `enum`, and `fn` signature array-length `TypeExpr`s.

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
fn rejects_non_channel_and_send_only_receives_in_struct_alias_enum_and_fn_signatures() {
    assert_eq!(
        codes(
            r#"import "fmt"
struct bad_struct {
    a [<-5]int
}
type bad_alias = [<-"abc"]int
enum bad_enum {
    v([<-true]int)
}
fn run(ch chan<- int, a [<-ch]int) [<-5]int {
    _ = ch
    return a
}
fn main() {
    fmt.println(1)
}
"#,
        ),
        vec!["HYB-E2008", "HYB-E2008", "HYB-E2008", "HYB-E2008", "HYB-E2008"],
        "non-channel and send-only receives in struct/alias/enum/fn signature array lengths must reject HYB-E2008"
    );
}
