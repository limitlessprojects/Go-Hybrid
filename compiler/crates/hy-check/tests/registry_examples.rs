//! registry_examples.rs — the `hyb explain` example lines are claims.
//!
//! Each `example` in `hy-diag/data/diagnostics.json` ends in
//! `// reject: HYB-Exxxx`: pasting it into a program must make the checker
//! emit that code. Five examples said otherwise (`var e option[int]` is
//! accepted — N3 gives option the zero value `none`; `fn f() error { return
//! nil }` resolves fine; `shape::circle => ...` is not Hybrid syntax;
//! `int8(300)` is folded per X4; `noop() <- 1` reports E3005) and five more
//! joined statements with `;`, which the parser rejects (LX9). This test
//! runs the registry text itself, so an example can no longer drift.

use hy_check::check_file;
use hy_diag::Registry;
use hy_syntax::parse_file;

/// The example's code lines: comments cut, nothing else touched.
fn example_code(code: &str) -> String {
    let entry = Registry::global().lookup(code).expect("registered code");
    entry
        .example
        .lines()
        .map(|l| l.split("//").next().unwrap().trim_end())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Leading `enum`/`fn` lines stay top-level; the rest runs inside `main`.
fn program(example: &str, top: &str, main_pre: &str) -> String {
    let mut decls = String::new();
    let mut body = String::new();
    let mut in_decl = true;
    let mut depth = 0i32;
    for line in example.lines() {
        let starts_decl = line.starts_with("enum ") || line.starts_with("fn ");
        if (in_decl && starts_decl) || depth > 0 {
            decls.push_str(line);
            decls.push('\n');
            depth += line.matches('{').count() as i32 - line.matches('}').count() as i32;
        } else {
            in_decl = false;
            body.push_str(line);
            body.push('\n');
        }
    }
    format!("{top}\n{decls}\nfn main() {{\n{main_pre}\n{body}}}\n")
}

fn fires(code: &str, top: &str, main_pre: &str) -> Result<(), String> {
    let src = program(&example_code(code), top, main_pre);
    let ast = parse_file("example.hy", &src)
        .map_err(|d| format!("{code}: example does not parse: {:?}", d.first().map(|d| &d.message)))?;
    let got: Vec<String> = check_file(&ast).diagnostics.iter().map(|d| d.code.clone()).collect();
    if got.iter().any(|c| c == code) {
        Ok(())
    } else {
        Err(format!("{code}: example does not fire it; checker said {got:?}"))
    }
}

#[test]
fn registry_examples_fire_their_code() {
    let parse = "fn parse(s string) (int, error) {\n    return 0, nil\n}";
    let shape = "enum shape { circle(f64)  square(f64)  triangle(f64) }";
    let failures: Vec<String> = [
        fires("HYB-E1001", "", ""),
        fires("HYB-E2008", "", ""),
        fires("HYB-E2009", "", ""),
        fires("HYB-E2011", parse, ""),
        fires(
            "HYB-E3003",
            "",
            "agg := map[string]int{}\nresults := make(chan map[string]int, 1)",
        ),
        fires("HYB-E3006", "", ""),
        fires("HYB-E3007", "", ""),
        fires("HYB-E4002", "", ""),
        fires("HYB-E5001", shape, "s := shape.circle(1.0)\nvar out f64"),
        fires("HYB-E6006", "", ""),
    ]
    .into_iter()
    .filter_map(Result::err)
    .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}
