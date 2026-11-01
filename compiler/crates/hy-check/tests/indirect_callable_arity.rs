//! BUILDABLE regressions: callable values retain their declared parameter count.

use hy_check::check_file;
use hy_syntax::parse_file;

fn diagnostics(src: &str) -> Vec<String> {
    let ast = parse_file("indirect.hy", src).expect("valid Hybrid syntax");
    check_file(&ast)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn function_fields_reject_wrong_argument_counts() {
    for call in ["h.f()", "h.f(1, 2)", "(h.f)(1, 2)"] {
        let source = format!(
            "struct holder {{ f fn(int) int }}\nfn apply(h holder) int {{\n return {call}\n}}\nfn main() {{}}\n"
        );
        assert_eq!(diagnostics(&source), ["HYB-E1002"], "{source}");
    }
}

#[test]
fn indexed_functions_reject_wrong_argument_counts() {
    for (ty, call) in [
        ("[]fn(int) int", "fs[0]()"),
        ("[]fn(int) int", "fs[0](1, 2)"),
        ("map[string]fn(int) int", "fs[\"f\"]()"),
        ("map[string]fn(int) int", "(fs[\"f\"])(1, 2)"),
    ] {
        let source = format!("fn apply(fs {ty}) int {{\n return {call}\n}}\nfn main() {{}}\n");
        assert_eq!(diagnostics(&source), ["HYB-E1002"], "{source}");
    }
}

#[test]
fn returned_functions_reject_wrong_argument_counts() {
    let source = "fn make_fn() fn(int) int {\n return fn(x int) int { return x }\n}\nfn main() {\n r := make_fn()(1, 2)\n fmt.println(r)\n}\n";
    assert_eq!(diagnostics(source), ["HYB-E1002"]);
}

#[test]
fn correctly_sized_indirect_calls_remain_accepted() {
    for source in [
        "struct holder { f fn(int) int }\nfn apply(h holder) int {\n return h.f(1)\n}\nfn main() {}\n",
        "fn apply(fs []fn(int) int) int {\n return fs[0](1)\n}\nfn main() {}\n",
        "fn apply(fs map[string]fn(int) int) int {\n return fs[\"f\"](1)\n}\nfn main() {}\n",
        "fn make_fn() fn(int) int {\n return fn(x int) int { return x }\n}\nfn main() {\n r := make_fn()(1)\n fmt.println(r)\n}\n",
    ] {
        assert!(diagnostics(source).is_empty(), "{source}");
    }
}

#[test]
fn tuple_forwarding_to_function_values_remains_accepted() {
    for source in [
        "fn pair() (int, int) { return 1, 2 }\nstruct holder { f fn(int, int) int }\nfn apply(h holder) int {\n return h.f(pair())\n}\nfn main() {}\n",
        "fn pair() (int, int) { return 1, 2 }\nfn apply(fs []fn(int, int) int) int {\n return fs[0](pair())\n}\nfn main() {}\n",
        "fn pair() (int, int) { return 1, 2 }\nfn apply(f fn(int, int) int) int {\n return f(pair())\n}\nfn main() {}\n",
    ] {
        assert!(diagnostics(source).is_empty(), "{source}");
    }
}

#[test]
fn wrong_sized_forwarded_tuples_are_rejected() {
    for source in [
        "fn pair() (int, int) { return 1, 2 }\nstruct holder { f fn(int, int, int) int }\nfn apply(h holder) int {\n return h.f(pair())\n}\nfn main() {}\n",
        "fn pair() (int, int) { return 1, 2 }\nfn apply(fs []fn(int, int, int) int) int {\n return fs[0](pair())\n}\nfn main() {}\n",
    ] {
        assert_eq!(diagnostics(source), ["HYB-E1002"], "{source}");
    }
}
