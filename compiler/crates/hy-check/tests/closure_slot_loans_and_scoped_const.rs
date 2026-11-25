//! Issue #2383: `check_closure_slot_loans` (`HYB-E3102`) tracks whole-map rebinds
//! during a live closure slot loan (`p := &m["a"]; m = ...; p.count = 10`) and
//! `scan_block_const_writes` (`HYB-E3006`) preserves `closure_scoped_envs` for
//! block-scoped post-closure const rebinds.

fn codes(src: &str) -> Vec<String> {
    let file = hy_syntax::parse_file("t.hy", src).expect("parse");
    hy_check::check_file(&file)
        .diagnostics
        .into_iter()
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn closure_multivalue_slot_loan_and_scoped_const_rebind_rejected() {
    let src1 = r#"struct svc {
    count int
}

fn main() {
    m := map[string]svc{"a": svc{count: 1}}
    f := fn() {
        s := &m["a"]
        m = map[string]svc{"a": svc{count: 2}}
        s.count = 10
    }
    f()
}
"#;
    assert_eq!(codes(src1), vec!["HYB-E3102"]);

    let src2 = r#"const const_map = map[string]int{"k": 1}

fn main() {
    if true {
        var alias map[string]int = map[string]int{"k": 2}
        mutator := fn() {
            alias["k"] = 999
        }
        alias = const_map
        mutator()
    }
}
"#;
    assert_eq!(codes(src2), vec!["HYB-E3006"]);
}