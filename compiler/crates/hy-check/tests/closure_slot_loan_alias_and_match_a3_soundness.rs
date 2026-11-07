//! Regression tests for closure map-slot loan (`HYB-E3102` A3) and loan-escape
//! (`HYB-E3104` O2) soundness across `move`, field-projected/composite map
//! aliases, `match some(...)` arm bindings, and `for ... in` iteration inside
//! closures.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("t.hy", src).expect("parse");
    check_file(&ast).diagnostics.into_iter().map(|d| d.code).collect()
}

#[test]
fn closure_move_of_loaned_map_preserves_a3_e3102() {
    let local_move = "struct svc { count int }\nfn main() {\n    f := fn() {\n        m := map[string]svc{\"a\": svc{count: 1}}\n        slot := &m[\"a\"]\n        m2 := move m\n        m2[\"b\"] = svc{count: 2}\n        slot.count = 10\n    }\n    f()\n}\n";
    assert!(codes(local_move).contains(&"HYB-E3102".to_string()));

    let assign_move_delete = "struct svc { count int }\nfn main() {\n    f := fn() {\n        m := map[string]svc{\"a\": svc{count: 1}}\n        slot := &m[\"a\"]\n        var m2 map[string]svc = map[string]svc{}\n        m2 = move m\n        delete(m2, \"a\")\n        slot.count = 10\n    }\n    f()\n}\n";
    assert!(codes(assign_move_delete).contains(&"HYB-E3102".to_string()));
}

#[test]
fn closure_projected_and_composite_map_aliases_trigger_a3_e3102() {
    let projected = "struct svc { count int }\nstruct holder { m map[string]svc }\nfn main() {\n    h := holder{m: map[string]svc{\"a\": svc{count: 1}}}\n    f := fn() {\n        slot := &h.m[\"a\"]\n        m2 := h.m\n        m2[\"b\"] = svc{count: 2}\n        slot.count = 10\n    }\n    f()\n}\n";
    assert_eq!(codes(projected), vec!["HYB-E3102"]);

    let composite = "struct svc { count int }\nstruct holder { m map[string]svc }\nfn main() {\n    m := map[string]svc{\"a\": svc{count: 1}}\n    f := fn() {\n        slot := &m[\"a\"]\n        h := holder{m: m}\n        h.m[\"b\"] = svc{count: 2}\n        slot.count = 10\n    }\n    f()\n}\n";
    assert_eq!(codes(composite), vec!["HYB-E3102"]);
}

#[test]
fn closure_match_bind_and_for_iterate_enforce_a3_e3102() {
    let match_slot_alias = "struct svc { count int }\nfn main() {\n    m := map[string]svc{\"a\": svc{count: 1}}\n    f := fn() {\n        slot := &m[\"a\"]\n        match some(slot) {\n            some(p) => {\n                m[\"b\"] = svc{count: 2}\n                p.count = 10\n            }\n            none => {}\n        }\n    }\n    f()\n}\n";
    assert_eq!(codes(match_slot_alias), vec!["HYB-E3102"]);

    let match_map_alias = "struct svc { count int }\nfn main() {\n    m := map[string]svc{\"a\": svc{count: 1}}\n    f := fn() {\n        opt := some(m)\n        slot := &m[\"a\"]\n        match opt {\n            some(m2) => {\n                m2[\"b\"] = svc{count: 2}\n            }\n            none => {}\n        }\n        slot.count = 10\n    }\n    f()\n}\n";
    assert_eq!(codes(match_map_alias), vec!["HYB-E3102"]);

    let for_iterate_alias = "struct svc { count int }\nfn main() {\n    ms := []map[string]svc{map[string]svc{\"a\": svc{count: 1}}}\n    f := fn() {\n        slot := &ms[0][\"a\"]\n        for _, m2 in ms {\n            m2[\"b\"] = svc{count: 2}\n        }\n        slot.count = 10\n    }\n    f()\n}\n";
    assert_eq!(codes(for_iterate_alias), vec!["HYB-E3102"]);
}

#[test]
fn closure_moved_and_match_slot_loan_return_rejected_with_e3104() {
    let move_return = "struct svc { count int }\nfn main() {\n    x := svc{count: 1}\n    f := fn() *svc {\n        p := &x\n        p2 := move p\n        return p2\n    }\n    _ = f()\n}\n";
    assert!(codes(move_return).contains(&"HYB-E3104".to_string()));

    let match_return = "struct svc { count int }\nfn main() {\n    x := svc{count: 1}\n    f := fn() *svc {\n        s := &x\n        match some(s) {\n            some(p) => {\n                return p\n            }\n            none => {\n                return &x\n            }\n        }\n    }\n    _ = f()\n}\n";
    assert!(codes(match_return).iter().filter(|c| *c == "HYB-E3104").count() >= 2);
}

#[test]
fn closure_post_window_alias_mutation_and_value_return_stay_accepted() {
    let legal = "struct svc { count int }\nstruct holder { m map[string]svc }\nfn main() {\n    h := holder{m: map[string]svc{\"a\": svc{count: 1}}}\n    f := fn() int {\n        slot := &h.m[\"a\"]\n        slot.count = 10\n        m2 := h.m\n        m2[\"b\"] = svc{count: 2}\n        return m2[\"a\"].count\n    }\n    _ = f()\n}\n";
    let ast = parse_file("t.hy", legal).expect("parse");
    let r = check_file(&ast);
    assert_eq!(r.verdict, Verdict::Accept, "unexpected diags: {:?}", r.diagnostics);
}
