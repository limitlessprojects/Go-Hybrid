//! Soundness regression suite for:
//! 1. O2 / LOAN-ESCAPE (`HYB-E3104`) on `BorrowHandle` and composite returns,
//!    plus A6 (`HYB-E3103`) field-borrow stores inside closures.
//! 2. A2 (`HYB-E3101`) borrow-handle loan window across `for u in v` and
//!    multi-target rebind assignments.
//! 3. A1b / O5.2 (`HYB-E3003`) loop back-edge use-after-move reachability via
//!    `continue loop` (LX14) inside nested non-infinite `for` loops.

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
}

fn assert_rejects_code(src: &str, code: &str) {
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject, "expected Reject, got: {:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == code),
        "expected {code}, got: {:?}",
        r.diagnostics
    );
}

#[test]
fn borrow_handle_return_escape_rejects_e3104() {
    // 1. Direct return of foreign borrow-handle call result.
    assert_rejects_code(
        "import \"rust/serde_json\"\n\
         fn leak_direct(raw string) (serde_json.view, error) {\n\
           doc := serde_json.parse(raw)?\n\
           return doc.array(\"users\")?, nil\n\
         }",
        "HYB-E3104",
    );

    // 2. Explicit return of local BorrowHandle variable `v`.
    assert_rejects_code(
        "import \"rust/serde_json\"\n\
         fn leak_bound(raw string) (serde_json.view, error) {\n\
           doc := serde_json.parse(raw)?\n\
           v := doc.array(\"users\")?\n\
           return v, nil\n\
         }",
        "HYB-E3104",
    );

    // 3. Explicit return of composite slice carrying a BorrowHandle.
    assert_rejects_code(
        "import \"rust/serde_json\"\n\
         fn leak_slice(raw string) ([]serde_json.view, error) {\n\
           doc := serde_json.parse(raw)?\n\
           v := doc.array(\"users\")?\n\
           return []serde_json.view{v}, nil\n\
         }",
        "HYB-E3104",
    );

    // 4. Bare return of named composite result carrying a BorrowHandle.
    assert_rejects_code(
        "import \"rust/serde_json\"\n\
         fn leak_named_map(raw string) (out map[string]serde_json.view, err error) {\n\
           doc := serde_json.parse(raw)?\n\
           v := doc.array(\"users\")?\n\
           out = map[string]serde_json.view{\"u\": v}\n\
           return\n\
         }",
        "HYB-E3104",
    );

    // 5. Returning an element view `u` from `for u in doc.array("users")?`.
    assert_rejects_code(
        "import \"rust/serde_json\"\n\
         fn leak_iter_elem(raw string) (serde_json.view, error) {\n\
           doc := serde_json.parse(raw)?\n\
           for u in doc.array(\"users\")? {\n\
             return u, nil\n\
           }\n\
           return doc.array(\"fallback\")?, nil\n\
         }",
        "HYB-E3104",
    );
}

#[test]
fn closure_field_store_of_bound_pointer_or_composite_borrow_rejects_e3103() {
    assert_rejects_code(
        "struct holder { p *int }\n\
         fn store_bound_ptr_in_closure(h *holder) {\n\
           f := fn() {\n\
             x := 42\n\
             p := &x\n\
             h.p = p\n\
           }\n\
           f()\n\
         }",
        "HYB-E3103",
    );

    assert_rejects_code(
        "struct holder { s []*int }\n\
         fn store_composite_ptr_in_closure() {\n\
           h := holder{s: []*int{}}\n\
           f := fn() {\n\
             x := 7\n\
             ps := []*int{}\n\
             ps = []*int{&x}\n\
             h.s = ps\n\
           }\n\
           f()\n\
           _ = h\n\
         }",
        "HYB-E3103",
    );
}

#[test]
fn a2_borrow_handle_loan_window_covers_for_iterate_and_multi_rebind() {
    // Mutating `doc` inside `for u in v` where `v := doc.array("users")?` rejects.
    assert_rejects_code(
        "import \"rust/serde_json\"\n\
         fn mutate_during_view_iter(raw string) (int, error) {\n\
           doc := serde_json.parse(raw)?\n\
           v := doc.array(\"users\")?\n\
           total := 0\n\
           for u in v {\n\
             doc.set(\"users\", 0)?\n\
             total += u.int(\"id\")?\n\
           }\n\
           return total, nil\n\
         }",
        "HYB-E3101",
    );

    // Positive control: mutating `doc` after `for u in v` ends accepts.
    let ok_after = check(
        "import \"rust/serde_json\"\n\
         fn mutate_after_view_iter(raw string) (int, error) {\n\
           doc := serde_json.parse(raw)?\n\
           v := doc.array(\"users\")?\n\
           total := 0\n\
           for u in v {\n\
             total += u.int(\"id\")?\n\
           }\n\
           doc.set(\"count\", total)?\n\
           return total, nil\n\
         }",
    );
    assert_eq!(ok_after.verdict, Verdict::Accept, "{:?}", ok_after.diagnostics);

    // Multi-value rebind (`v, ok = m["u"]`) extends the loan on `doc`.
    assert_rejects_code(
        "import \"rust/serde_json\"\n\
         fn mutate_after_multi_rebind(raw string) (int, error) {\n\
           doc := serde_json.parse(raw)?\n\
           m := map[string]serde_json.view{\"u\": doc.array(\"users\")?}\n\
           v := doc.array(\"init\")?\n\
           ok := false\n\
           v, ok = m[\"u\"]\n\
           doc.set(\"users\", 0)?\n\
           if ok {\n\
             return v.int(\"id\")?, nil\n\
           }\n\
           return 0, nil\n\
         }",
        "HYB-E3101",
    );
}

#[test]
fn loop_backedge_catches_labeled_continue_loop_inside_nested_for() {
    assert_rejects_code(
        "fn worker(m map[string]int, done chan<- bool) {\n\
           m[\"worker\"] = 1\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           m := map[string]int{\"init\": 0}\n\
           done := make(chan bool, 2)\n\
           step := 0\n\
           loop {\n\
             m[\"main\"] = step\n\
             task worker(move m, done)\n\
             for j in 0..1 {\n\
               _ = j\n\
               if step == 0 {\n\
                 step = 1\n\
                 continue loop\n\
               }\n\
             }\n\
             break\n\
           }\n\
           <-done\n\
         }",
        "HYB-E3003",
    );

    assert_rejects_code(
        "fn worker(m map[string]int, done chan<- bool) {\n\
           m[\"worker\"] = 1\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           m := map[string]int{\"init\": 0}\n\
           done := make(chan bool, 2)\n\
           step := 0\n\
           loop {\n\
             m[\"main\"] = step\n\
             for j in 0..1 {\n\
               _ = j\n\
               task worker(move m, done)\n\
               if step == 0 {\n\
                 step = 1\n\
                 continue loop\n\
               }\n\
             }\n\
             return\n\
           }\n\
         }",
        "HYB-E3003",
    );
}
