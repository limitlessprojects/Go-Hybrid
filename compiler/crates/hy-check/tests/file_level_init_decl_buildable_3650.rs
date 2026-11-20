use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn file_level_non_fn_init_declarations_reject_3650() {
    let cases = [
        ("package main\nimport \"fmt\"\nconst init = 1\nfn main() { fmt.println(2) }\n", "const init"),
        ("package main\nimport \"fmt\"\ntype init = int\nfn main() { fmt.println(2) }\n", "type init"),
        ("package main\nimport \"fmt\"\nstruct init { x int }\nfn main() { fmt.println(2) }\n", "struct init"),
        ("package main\nimport \"fmt\"\nenum init { a b }\nfn main() { fmt.println(2) }\n", "enum init"),
    ];

    for (src, label) in cases {
        let r = check_file(&parse_file("repro.hy", src).unwrap());
        assert_eq!(
            r.verdict,
            Verdict::Reject,
            "case {label} must reject with HYB-E1002: {:?}",
            r.diagnostics
        );
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
            "case {label} must emit HYB-E1002: {:?}",
            r.diagnostics
        );
    }

    // Negative control: legal fn init and function-local init variable must accept
    let accept_src = r#"
package main
import "fmt"

fn init() {}

fn main() {
    init := 2
    fmt.println(init)
}
"#;
    let r = check_file(&parse_file("accept.hy", accept_src).unwrap());
    assert_eq!(r.verdict, Verdict::Accept, "legal init fn and local init must accept: {:?}", r.diagnostics);
}
