//! alias_struct_lit_share.rs — regression test for #385 (part 1): a struct
//! literal spelled through a type alias (`holder_alias{m: m1}`) used to report
//! `Struct(Some("holder_alias"))`, which `shape_may_share` cannot resolve
//! (`self.structs` is keyed by the real name). The copy `h2 := h` therefore
//! recorded no may-share edge and moving `h` into a task left `h2` free to
//! mutate the same map with 0 diagnostics.

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src).unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

const ALIASED_HOLDER: &str = r#"
struct holder {
    m map[string]int
}
type holder_alias = holder

fn worker_struct(h holder_alias, done chan<- bool) {
    h.m["k"] = 1
    done <- true
}

fn main() {
    done := make(chan bool, 1)
    m1 := map[string]int{"k": 0}
    h := holder_alias{m: m1}
    h2 := h
    task worker_struct(move h, done)
    h2.m["k"] = 2
    <-done
}
"#;

#[test]
fn aliased_struct_literal_copy_is_a_share_peer() {
    let got = codes(&check(ALIASED_HOLDER));
    assert!(
        got.iter().any(|c| c == "HYB-E3003" || c == "HYB-E3004"),
        "expected a move/share diagnostic for the aliased literal copy, got {:?}",
        got
    );
}

#[test]
fn direct_struct_literal_copy_still_reports() {
    // The control: the same program written with the struct name keeps
    // behaving exactly as before the alias fix.
    let src = ALIASED_HOLDER.replace("holder_alias{m: m1}", "holder{m: m1}")
        .replace("h holder_alias", "h holder");
    let got = codes(&check(&src));
    assert!(
        got.iter().any(|c| c == "HYB-E3003" || c == "HYB-E3004"),
        "the direct spelling must keep its diagnostic, got {:?}",
        got
    );
}
