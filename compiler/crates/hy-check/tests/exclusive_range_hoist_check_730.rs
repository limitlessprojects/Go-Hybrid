use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn has_e2009(src: &str) -> bool {
    let f = parse_file("test.hy", src).expect("parse succeeds");
    check_file(&f).diagnostics.iter().any(|d| d.code == "HYB-E2009")
}

fn assert_accept(src: &str) {
    let f = parse_file("test.hy", src).expect("parse succeeds");
    let out = check_file(&f);
    assert_eq!(out.verdict, Verdict::Accept, "expected Accept, got {:?}", out.diagnostics);
}

#[test]
fn reject_typed_or_rune_bound_mismatch_when_shadowing_and_accept_matching_730() {
    assert!(has_e2009(
        "package main\nconst n = 10\nfn main() { var start int64 = 5\nfor n in start..n {} }"
    ));
    assert!(has_e2009(
        "package main\nfn main() { var m int64 = 4\nfor m in 0..m {} }"
    ));
    assert!(has_e2009(
        "package main\nconst c = 'z'\nfn main() { for c in 0..c {} }"
    ));
    assert!(has_e2009(
        "package main\nconst a = 'z'\nconst d = a\nfn main() { for d in 0..d {} }"
    ));
    assert!(has_e2009(
        "package main\nconst a = 'z'\nconst d = a\nfn main() { a := 1\n_ = a\nfor d in 0..d {} }"
    ));
    assert!(has_e2009(
        "package main\nconst c = 'a' + 25\nfn main() { for c in 0..c + 1 {} }"
    ));
    assert!(has_e2009(
        "package main\nfn main() { c := 200\nfor c in 'a'..c { _ = c } }"
    ));
    assert!(has_e2009(
        "package main\nconst c = 'z'\nfn main() { c := 200\nfor c in 'a'..c + 1 { _ = c } }"
    ));
    assert!(has_e2009(
        "package main\nconst z = 'a' + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1 + 1\nfn main() { for z in 0..z {} }"
    ));

    assert_accept(
        r#"package main
const n = 10
const c = 'z'
const s = 1 << ('d' - 'a')
const kt int = 5
const z_typed = 'a' + kt
fn main() {
    for n in 0..n {}
    for n in 0..=n {}
    for c in 'a'..c {}
    for c in 'a'..=c {}
    for s in 0..s {}
    for z_typed in 0..z_typed {}
    c_loc := 5
    for c_loc in 0..c_loc + 1 {}
    c := 5
    for c in 0..c + 1 {}
    k := 3
    for k in 0..k * 2 {}
    for k in 0..'a' + k {}
    var m int64 = 4
    for m in int64(0)..m {}
    for m in int64(0)..m + 'a' {}
    var m32 int32 = 100
    for m32 in 'a'..m32 {}
    var start int64 = 5
    for i in start..10 { _ = i }
    for i in 0..'z' { _ = i }
    for i in 0..c { _ = i }
    var ni int = 0
    for i in ni..'z' { _ = i }
    for i in ni..=ni + 'a' { _ = i }
    for r in 'a'..'z' { _ = r }
    for r in 'a'..='z' { _ = r }
}
"#,
    );
}
