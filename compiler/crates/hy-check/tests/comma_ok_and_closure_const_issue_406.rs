use hy_check::{check_file, CheckResult, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let file = parse_file("test.hy", src).expect("parse");
    check_file(&file)
}

#[test]
fn comma_ok_map_lookup_move_freezes_outer_map() {
    let r = check(
        "fn worker(inner map[string]int, done chan<- bool) {\n\
           inner[\"k\"] = 1\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           m := map[string]map[string]int{\"a\": map[string]int{\"k\": 0}}\n\
           v, ok := m[\"a\"]\n\
           done := make(chan bool, 1)\n\
           if ok {\n\
             task worker(move v, done)\n\
           }\n\
           m[\"a\"][\"k\"] = 2\n\
           <-done\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 on outer map write after moving comma-ok map value, got {:?}",
        r.diagnostics
    );
}

#[test]
fn comma_ok_map_lookup_on_const_rejects_write_and_move() {
    let r = check(
        "const shared = map[string]map[string]int{\"a\": map[string]int{\"k\": 0}}\n\
         fn worker(inner map[string]int, done chan<- bool) {\n\
           inner[\"k\"] = 1\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           v, ok := shared[\"a\"]\n\
           done := make(chan bool, 1)\n\
           if ok {\n\
             v[\"k\"] = 42\n\
             task worker(move v, done)\n\
             <-done\n\
           }\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006 on write through comma-ok projection of const map, got {:?}",
        r.diagnostics
    );
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
        "expected HYB-E3004 on move of comma-ok projection of const map, got {:?}",
        r.diagnostics
    );
}

#[test]
fn comma_ok_map_lookup_rebind_and_a3_loan_rejects() {
    let r = check(
        "import \"fmt\"\n\
         struct entry { count int }\n\
         fn main() {\n\
           m := map[string]map[string]entry{\"a\": map[string]entry{\"x\": entry{count: 1}}}\n\
           var inner map[string]entry = map[string]entry{}\n\
           var ok bool = false\n\
           inner, ok = m[\"a\"]\n\
           if ok {\n\
             slot := &inner[\"x\"]\n\
             delete(m[\"a\"], \"x\")\n\
             fmt.println(slot.count)\n\
           }\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3102"),
        "expected HYB-E3102 when deleting from outer map during slot loan on comma-ok peer, got {:?}",
        r.diagnostics
    );
}

#[test]
fn closure_local_borrow_and_move_define_of_const_reject_with_e3006() {
    let r1 = check(
        "struct counter { n int }\n\
         const shared = counter{n: 0}\n\
         fn main() {\n\
           corrupt := fn() {\n\
             p := &shared\n\
             p.n = 99\n\
           }\n\
           corrupt()\n\
         }",
    );
    assert_eq!(r1.verdict, Verdict::Reject, "{:?}", r1.diagnostics);
    assert!(
        r1.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006 for closure local pointer borrow write to const struct, got {:?}",
        r1.diagnostics
    );

    let r2 = check(
        "const shared_map = map[string]int{\"k\": 0}\n\
         fn main() {\n\
           corrupt := fn() {\n\
             m1 := shared_map\n\
             m2 := move m1\n\
             m2[\"k\"] = 99\n\
           }\n\
           corrupt()\n\
         }",
    );
    assert_eq!(r2.verdict, Verdict::Reject, "{:?}", r2.diagnostics);
    assert!(
        r2.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006 for closure move-define alias write to const map, got {:?}",
        r2.diagnostics
    );

    let r3 = check(
        "const shared = map[string]map[string]int{\"a\": map[string]int{\"k\": 0}}\n\
         fn main() {\n\
           corrupt := fn() {\n\
             v, ok := shared[\"a\"]\n\
             if ok {\n\
               v[\"k\"] = 77\n\
             }\n\
           }\n\
           corrupt()\n\
         }",
    );
    assert_eq!(r3.verdict, Verdict::Reject, "{:?}", r3.diagnostics);
    assert!(
        r3.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006 for closure comma-ok lookup on const map, got {:?}",
        r3.diagnostics
    );
}

#[test]
fn comma_ok_scalar_map_lookup_still_accepts() {
    let r = check(
        "import \"fmt\"\n\
         fn worker(m map[string]int, done chan<- bool) {\n\
           m[\"k\"] = 2\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           m := map[string]int{\"k\": 1}\n\
           v, ok := m[\"k\"]\n\
           done := make(chan bool, 1)\n\
           task worker(move m, done)\n\
           if ok {\n\
             fmt.println(v)\n\
           }\n\
           <-done\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
}
