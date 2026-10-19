use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn first_error_rule(r: &hy_check::CheckResult) -> Option<String> {
    r.diagnostics
        .iter()
        .find(|d| d.severity == Severity::Error)
        .and_then(|d| d.tier0_rule.clone())
}

#[test]
fn loop_call_local_alias_move_rejects_on_backedge_e3003_issue_194() {
    // Issue #194: `m := same(agg)` inside a loop aliases `agg`; moving `m`
    // in the loop freezes `agg` across the backedge and must reject on the
    // second iteration with HYB-E3003 (A1b).
    let r = check(
        "fn same(m map[string]int) map[string]int {\n\
           return m\n\
         }\n\
         fn worker(m map[string]int, done chan<- bool) {\n\
           for i in 0..100 {\n\
             m[\"k\"] = i\n\
           }\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           agg := map[string]int{\"k\": 0}\n\
           done := make(chan bool, 10)\n\
           for i in 0..2 {\n\
             m := same(agg)\n\
             task worker(move m, done)\n\
           }\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(first_error_rule(&r).as_deref(), Some("A1b"));
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003, got {:?}",
        r.diagnostics
    );
}

#[test]
fn loop_call_fresh_append_copy_still_accepts_corpus_15_shape() {
    // Positive control (corpus #15): a helper that returns `append([]string{}, ...)`
    // allocates a fresh slice of value/string elements per iteration, so moving
    // `chunk` in a loop remains legal.
    let r = check(
        "fn chunk_of(words []string, i int, n int) []string {\n\
           size := (len(words) + n - 1) / n\n\
           a := i * size\n\
           b := min(a + size, len(words))\n\
           return append([]string{}, words[a:b]...)\n\
         }\n\
         fn tally(shard []string, results chan<- int) {\n\
           results <- len(shard)\n\
         }\n\
         fn main() {\n\
           words := []string{\"a\", \"b\", \"c\", \"d\"}\n\
           results := make(chan int, 2)\n\
           for i in 0..2 {\n\
             chunk := chunk_of(words, i, 2)\n\
             task tally(move chunk, results)\n\
           }\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn loop_fresh_copy_local_does_not_erase_move_of_outer_slice_on_backedge() {
    // Defining `snap := copy_slice(xs)` inside a loop must not erase `b.moved`
    // on `xs` when `xs` itself (or a real alias) is moved in the loop.
    let r = check(
        "fn copy_slice(xs []int) []int {\n\
           return append([]int{}, xs...)\n\
         }\n\
         fn worker(xs []int, done chan<- bool) {\n\
           for i in 0..100 {\n\
             xs[0] = i\n\
           }\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           xs := []int{1, 2, 3}\n\
           done := make(chan bool, 2)\n\
           for i in 0..2 {\n\
             snap := copy_slice(xs)\n\
             if len(snap) > 0 {\n\
               task worker(move xs, done)\n\
             }\n\
           }\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(first_error_rule(&r).as_deref(), Some("A1b"));
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003, got {:?}",
        r.diagnostics
    );
}

#[test]
fn loop_trailing_break_or_return_with_reachable_continue_rejects_on_backedge() {
    // A trailing `break` or `return` at the end of a loop body does not prevent
    // repetition when an earlier branch executes `continue` targeting that loop.
    for tail in ["break", "return"] {
        let src = format!(
            "fn worker(m map[string]int, done chan<- bool) {{\n\
               for i in 0..100 {{\n\
                 m[\"k\"] = i\n\
               }}\n\
               done <- true\n\
             }}\n\
             fn main() {{\n\
               m := map[string]int{{\"k\": 0}}\n\
               done := make(chan bool, 2)\n\
               for i in 0..2 {{\n\
                 task worker(move m, done)\n\
                 if i == 0 {{\n\
                   continue\n\
                 }}\n\
                 {tail}\n\
               }}\n\
             }}"
        );
        let r = check(&src);
        assert_eq!(r.verdict, Verdict::Reject, "tail={tail}: {:?}", r.diagnostics);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1b"));
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
            "tail={tail}: expected HYB-E3003, got {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn defer_overwriting_named_result_is_not_treated_as_fresh_copy() {
    // A helper with a named result and a `defer` that overwrites that result
    // after `return append([]int{}, xs...)` returns an alias of `xs`, not a
    // fresh copy, and must reject when `xs` is moved and the returned slice is
    // subsequently read.
    let r = check(
        "type ints = []int\n\
         fn copy_slice(xs []int) (out ints) {\n\
           defer fn() {\n\
             out = xs\n\
           }()\n\
           return append([]int{}, xs...)\n\
         }\n\
         fn worker(xs []int, done chan<- bool) {\n\
           for i in 0..100 {\n\
             xs[0] = i\n\
           }\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           xs := []int{1, 2, 3}\n\
           done := make(chan bool, 1)\n\
           snap := copy_slice(xs)\n\
           task worker(move xs, done)\n\
           println(snap[0])\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(first_error_rule(&r).as_deref(), Some("A1b"));
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003, got {:?}",
        r.diagnostics
    );
}

#[test]
fn slice_of_struct_with_unknown_any_field_is_not_treated_as_fresh_copy() {
    // A struct element carrying an unresolved/`any` field (`struct box { data any }`)
    // can wrap a mutable reference (such as a `map[string]int`), so
    // `append([]box{}, xs...)` shallow-copies the element and still shares the
    // nested reference storage.
    let r = check(
        "struct box {\n\
           data any\n\
         }\n\
         fn copy_boxes(xs []box) []box {\n\
           return append([]box{}, xs...)\n\
         }\n\
         fn worker(xs []box, done chan<- bool) {\n\
           for i in 0..100 {\n\
             xs[0] = box{data: i}\n\
           }\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           m := map[string]int{\"k\": 0}\n\
           xs := []box{box{data: m}}\n\
           done := make(chan bool, 1)\n\
           snap := copy_boxes(xs)\n\
           task worker(move xs, done)\n\
           println(len(snap))\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(first_error_rule(&r).as_deref(), Some("A1b"));
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003, got {:?}",
        r.diagnostics
    );
}

#[test]
fn continue_before_move_then_break_or_return_accepts() {
    // When `continue` only appears before the move site and execution after the
    // move site unconditionally exits the loop (`break` or `return`), the move
    // executes at most once across the loop and must be accepted.
    for tail in ["break", "return"] {
        let src = format!(
            "fn worker(xs []int, done chan<- bool) {{\n\
               done <- len(xs) > 0\n\
             }}\n\
             fn main() {{\n\
               xs := []int{{1, 2, 3}}\n\
               done := make(chan bool, 1)\n\
               for i in 0..2 {{\n\
                 if i == 0 {{\n\
                   continue\n\
                 }}\n\
                 task worker(move xs, done)\n\
                 {tail}\n\
               }}\n\
             }}"
        );
        let r = check(&src);
        assert_eq!(r.verdict, Verdict::Accept, "tail={tail}: {:?}", r.diagnostics);
    }
}

#[test]
fn self_named_fresh_copy_local_in_loop_accepts() {
    // In `copy_slice := copy_slice(xs)`, the RHS evaluates the top-level helper
    // `copy_slice` before the new local binding `copy_slice` comes into scope.
    let r = check(
        "fn copy_slice(xs []int) []int {\n\
           return append([]int{}, xs...)\n\
         }\n\
         fn worker(xs []int, done chan<- bool) {\n\
           done <- len(xs) > 0\n\
         }\n\
         fn main() {\n\
           xs := []int{1, 2, 3}\n\
           done := make(chan bool, 2)\n\
           for i in 0..2 {\n\
             copy_slice := copy_slice(xs)\n\
             task worker(move copy_slice, done)\n\
           }\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}

#[test]
fn self_named_non_fresh_call_still_rejects_on_backedge() {
    // Conversely, when the helper is NOT a fresh copy (`same := same(m)`),
    // the local binding still aliases `m` and moving it in a loop rejects.
    let r = check(
        "fn same(m map[string]int) map[string]int {\n\
           return m\n\
         }\n\
         fn worker(m map[string]int, done chan<- bool) {\n\
           m[\"k\"] = 1\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           m := map[string]int{\"k\": 0}\n\
           done := make(chan bool, 2)\n\
           for i in 0..2 {\n\
             same := same(m)\n\
             task worker(move same, done)\n\
           }\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(first_error_rule(&r).as_deref(), Some("A1b"));
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003, got {:?}",
        r.diagnostics
    );
}

#[test]
fn select_header_consume_with_reachable_default_arm_rejects_on_backedge() {
    // Go evaluates `select` send values before choosing an arm, so a consuming
    // call in a `select` send header (`ch <- len(enc.finish()?)`) executes on
    // every iteration even when that arm's body breaks out of the loop and
    // `default` continues to the next iteration.
    let r = check(
        "import \"rust/flate2\"\n\
         fn main() {\n\
           enc := flate2.new_writer(6)?\n\
           ch := make(chan int)\n\
           for i in 0..2 {\n\
             select {\n\
               ch <- len(enc.finish()?): {\n\
                 break loop\n\
               }\n\
               default: {}\n\
             }\n\
           }\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert_eq!(first_error_rule(&r).as_deref(), Some("A7"));
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003, got {:?}",
        r.diagnostics
    );
}

#[test]
fn parallel_fresh_shadow_preserves_outer_alias_for_sibling_rhs() {
    // In `xs, snap := copy_slice(xs), xs`, Go evaluates both RHS expressions
    // against the outer `xs` before introducing the shadowed `xs`. Moving `snap`
    // in a loop or while outer `xs` is read later must still reject.
    let r = check(
        "fn copy_slice(xs []int) []int {\n\
           return append([]int{}, xs...)\n\
         }\n\
         fn worker(xs []int, done chan<- bool) {\n\
           xs[0] = 9\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           xs := []int{1, 2, 3}\n\
           done := make(chan bool, 2)\n\
           for i in 0..2 {\n\
             xs, snap := copy_slice(xs), xs\n\
             _ = xs\n\
             task worker(move snap, done)\n\
           }\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003, got {:?}",
        r.diagnostics
    );
}

#[test]
fn parallel_fresh_shadow_in_closure_preserves_outer_alias() {
    let r = check(
        "fn copy_slice(xs []int) []int {\n\
           return append([]int{}, xs...)\n\
         }\n\
         fn worker(xs []int, done chan<- bool) {\n\
           xs[0] = 9\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           done := make(chan bool, 2)\n\
           run := fn() {\n\
             xs := []int{1, 2, 3}\n\
             if true {\n\
               xs, snap := copy_slice(xs), xs\n\
               _ = xs\n\
               task worker(move snap, done)\n\
             }\n\
             _ = xs[0]\n\
           }\n\
           run()\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
}

#[test]
fn parallel_fresh_rebind_preserves_sibling_alias() {
    let r = check(
        "fn copy_slice(xs []int) []int {\n\
           return append([]int{}, xs...)\n\
         }\n\
         fn worker(xs []int, done chan<- bool) {\n\
           xs[0] = 9\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           xs := []int{1, 2, 3}\n\
           keep := xs\n\
           snap := []int{0}\n\
           done := make(chan bool, 2)\n\
           xs, snap = copy_slice(xs), xs\n\
           _ = xs\n\
           task worker(move snap, done)\n\
           _ = keep[0]\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
}

#[test]
fn multi_return_shadow_preserves_outer_alias() {
    let r = check(
        "fn int_and_slice(xs []int) (int, []int) {\n\
           return 1, xs\n\
         }\n\
         fn worker(xs []int, done chan<- bool) {\n\
           xs[0] = 9\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           xs := []int{1, 2, 3}\n\
           done := make(chan bool, 2)\n\
           if true {\n\
             xs, snap := int_and_slice(xs)\n\
             _ = xs\n\
             task worker(move snap, done)\n\
           }\n\
           _ = xs[0]\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003, got {:?}",
        r.diagnostics
    );
}

#[test]
fn sequential_map_aliases_with_and_without_prior_move() {
    for prior_move in [false, true] {
        let transfer = if prior_move { "task worker(move original)" } else { "" };
        let r = check(&format!(
            "fn worker(m map[string]int) {{ m[\"k\"] = 1 }}
             fn main() {{
               original := map[string]int{{\"k\": 0}}
               {transfer}
               alias := original
               next := alias
               task worker(move next)
               _ = original[\"k\"]
             }}"
        ));
        assert_eq!(r.verdict, Verdict::Reject, "prior_move={prior_move}: {:?}", r.diagnostics);
        assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E3001" || d.code == "HYB-E3003"), "{:?}", r.diagnostics);
    }
    let r = check(
        "fn main() {
           original := map[string]int{\"k\": 0}
           alias := original
           next := alias
           next[\"k\"] = 1
           _ = original[\"k\"]
         }",
    );
    assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
}
