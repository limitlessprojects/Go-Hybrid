//! Conversions to a named slice, map or channel type need identical element
//! (key/value) types (issue #1840, rule BUILDABLE): `my_slice(ints)` with
//! `type my_slice = []string` is accepted by `hyb check` but rejected by
//! `go build`.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("t.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn rejects_e1002(src: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "{src}");
    assert!(
        r.diagnostics
            .iter()
            .any(|d| d.code == "HYB-E1002" && d.message.contains("cannot convert")),
        "{:?}",
        r.diagnostics
    );
}

#[test]
fn slice_with_other_element_type_rejects() {
    rejects_e1002(
        "type my_slice = []string\n\nfn main() {\n    var s []int = []int{1, 2, 3}\n    _ = my_slice(s)\n}\n",
    );
}

#[test]
fn map_with_swapped_key_and_value_rejects() {
    rejects_e1002(
        "type my_map = map[string]int\n\nfn main() {\n    var m map[int]string\n    _ = my_map(m)\n}\n",
    );
}

#[test]
fn map_with_other_value_type_rejects() {
    rejects_e1002(
        "type my_map = map[string]int\n\nfn main() {\n    var m map[string]bool\n    _ = my_map(m)\n}\n",
    );
}

#[test]
fn chan_with_other_element_type_rejects() {
    rejects_e1002(
        "type my_chan = chan string\n\nfn main() {\n    var c chan int\n    _ = my_chan(c)\n}\n",
    );
}

#[test]
fn matching_container_conversions_still_accepted() {
    let src = "type my_slice = []string\ntype my_map = map[string]int\ntype my_chan = chan string\n\nfn main() {\n    var s []string = []string{\"a\"}\n    var m map[string]int\n    var c chan string\n    _ = my_slice(s)\n    _ = my_map(m)\n    _ = my_chan(c)\n}\n";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}
