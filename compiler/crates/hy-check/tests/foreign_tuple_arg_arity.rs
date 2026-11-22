//! Foreign fixed-arity calls count a sole tuple-valued argument by its results.
use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn result(src: &str) -> hy_check::CheckResult {
    check_file(&parse_file("foreign-tuple.hy", src).unwrap_or_else(|d| panic!("{src}: {d:?}")))
}

#[test]
fn valid_multi_result_arguments_match_fixed_go_signatures() {
    let cases = [
        (
            "path",
            "fn pair() (string, string) { return \"a\", \"b\" }",
            "path.match(pair())",
        ),
        (
            "path/filepath",
            "fn pair() (string, string) { return \"a\", \"b\" }",
            "filepath.rel(pair())",
        ),
        (
            "net/url",
            "fn pair() (string, string) { return \"a\", \"b\" }",
            "url.user_password(pair())",
        ),
        (
            "strconv",
            "fn pair() (i64, int) { return 1, 10 }",
            "strconv.format_int(pair())",
        ),
        (
            "strconv",
            "fn triple() (string, int, int) { return \"1\", 10, 64 }",
            "strconv.parse_int(triple())",
        ),
        (
            "strconv",
            "fn four() (f64, byte, int, int) { return 1.0, 103, -1, 64 }",
            "strconv.format_float(four())",
        ),
        (
            "strconv",
            "fn five() ([]byte, f64, byte, int, int) { return nil, 1.0, 103, -1, 64 }",
            "strconv.append_float(five())",
        ),
        (
            "sort",
            "fn pair() ([]int, int) { return nil, 1 }",
            "sort.search_ints(pair())",
        ),
        (
            "encoding/hex",
            "fn pair() ([]byte, []byte) { return nil, nil }",
            "hex.decode(pair())",
        ),
        (
            "io",
            "fn pair() (io.writer, io.reader) { return nil, nil }",
            "io.copy(pair())",
        ),
    ];
    let mut failures = Vec::new();
    for (import, decl, call) in cases {
        let src = format!("import \"{import}\"\n{decl}\nfn main() {{ {call} }}");
        let r = result(&src);
        if r.verdict != Verdict::Accept {
            failures.push(format!("{call}: {:?}", r.diagnostics));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn unary_foreign_calls_reject_two_forwarded_results() {
    let cases = [
        ("path", "path.clean"),
        ("path/filepath", "filepath.clean"),
        ("net/url", "url.query_escape"),
        ("strconv", "strconv.quote"),
        ("sort", "sort.ints"),
        ("encoding/hex", "hex.encode_to_string"),
        ("sync", "sync.once_func"),
        ("io", "io.read_all"),
    ];
    let mut failures = Vec::new();
    for (import, call) in cases {
        let src = format!("import \"{import}\"\nfn pair() (string, string) {{ return \"a\", \"b\" }}\nfn main() {{ {call}(pair()) }}");
        let r = result(&src);
        if r.verdict != Verdict::Reject || !r.diagnostics.iter().any(|d| d.code == "HYB-E1002") {
            failures.push(format!("{call}: {:?}", r.diagnostics));
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn wrong_expanded_counts_reject_even_when_the_written_list_has_one_argument() {
    for (import, call) in [
        ("path", "path.match"),
        ("net/url", "url.user_password"),
        ("strconv", "strconv.format_int"),
        ("sort", "sort.search_ints"),
        ("encoding/hex", "hex.decode"),
    ] {
        let src = format!("import \"{import}\"\nfn triple() (string, string, string) {{ return \"a\", \"b\", \"c\" }}\nfn main() {{ {call}(triple()) }}");
        let r = result(&src);
        assert_eq!(r.verdict, Verdict::Reject, "{call}: {:?}", r.diagnostics);
        assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E1002"));
    }
}

#[test]
fn ordinary_and_variadic_calls_keep_their_verdicts() {
    for src in [
        "import \"path\"\nfn main() { path.match(\"a\", \"b\") }",
        "import \"net/url\"\nfn main() { url.join_path(\"a\", \"b\") }",
        "import \"path/filepath\"\nfn main() { filepath.join(\"a\", \"b\") }",
    ] {
        let r = result(src);
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    }
    let r = result("import \"path\"\nfn main() { path.clean([]string{\"a\"}...) }");
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E1002"));
}
