//! Regression tests for #1343: string-index assignment/loan (`s[0] = v`, `&s[0]`,
//! including through `match` option binders) must reject as HYB-E2010.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn rejects_with(src: &str, code: &str) {
    let r = check(src);
    let cs = error_codes(&r);
    assert!(
        cs.iter().any(|c| c == code),
        "expected {}, got {:?}\nsrc:\n{}",
        code,
        cs,
        src
    );
    assert_eq!(r.verdict, Verdict::Reject, "src:\n{}", src);
}

#[test]
fn rejects_string_index_store_and_loan_including_match_option_binder() {
    rejects_with(
        "fn main() {\n\
            s := \"hello\"\n\
            s[0] = 65\n\
            n := &s[0]\n\
            _ = n\n\
            _ = s\n\
        }\n",
        "HYB-E2010",
    );
    rejects_with(
        "fn main() {\n\
            var opt option[string] = some(\"hello\")\n\
            match opt {\n\
                some(s): {\n\
                    s[0] = 65\n\
                    n := &s[0]\n\
                    _ = n\n\
                    _ = s\n\
                }\n\
                none: {}\n\
            }\n\
        }\n",
        "HYB-E2010",
    );
}

fn e2010_messages(src: &str) -> Vec<String> {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "src:\n{}", src);
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error && d.code == "HYB-E2010")
        .map(|d| d.message.clone())
        .collect()
}

#[test]
fn string_byte_store_and_loan_name_the_string_not_a_map_element() {
    let msgs = e2010_messages(
        "fn main() {\n\
            s := \"hello\"\n\
            s[0] = 65\n\
            _ = s\n\
        }\n",
    );
    assert_eq!(msgs.len(), 1, "{:?}", msgs);
    assert!(msgs[0].contains("Go strings are immutable"), "{:?}", msgs);
    assert!(!msgs[0].contains("map element"), "{:?}", msgs);
    let r = check(
        "fn main() {
            s := \"hello\"
            s[0] = 65
            _ = s
        }
",
    );
    let d = r
        .diagnostics
        .iter()
        .find(|d| d.code == "HYB-E2010")
        .expect("E2010");
    assert!(
        d.fixits.iter().all(|f| !f.description.contains("m[k]")),
        "string byte fix-its must not suggest map routes: {:?}",
        d.fixits
    );
    assert!(
        d.fixits.iter().any(|f| f.description.contains("[]byte")),
        "{:?}",
        d.fixits
    );

    let msgs = e2010_messages(
        "struct user { name string }\n\
        fn main() {\n\
            u := user{name: \"bo\"}\n\
            u.name[0] = 66\n\
            n := &u.name[1]\n\
            _ = n\n\
            _ = u\n\
        }\n",
    );
    assert!(msgs.len() >= 2, "{:?}", msgs);
    for m in &msgs {
        assert!(m.contains("Go strings are immutable"), "{:?}", msgs);
        assert!(!m.contains("map element"), "{:?}", msgs);
    }
}

#[test]
fn map_element_store_still_names_the_map_element() {
    let msgs = e2010_messages(
        "struct p { n int }\n\
        fn main() {\n\
            m := map[string]p{\"a\": p{n: 1}}\n\
            m[\"a\"].n = 2\n\
            _ = m\n\
        }\n",
    );
    assert_eq!(msgs.len(), 1, "{:?}", msgs);
    assert!(msgs[0].contains("map element"), "{:?}", msgs);
    assert!(!msgs[0].contains("strings are immutable"), "{:?}", msgs);
}
