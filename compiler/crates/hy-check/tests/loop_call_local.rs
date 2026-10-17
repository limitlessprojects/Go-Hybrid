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
fn loop_fresh_copy_does_not_clear_backedge_move_of_outer_slice_or_alias_e3003() {
    // Regression: `call_local_roots` in `check_loop_backedge` resolved the
    // fresh-copy local `chunk := chunk_of(words)` through `st.aliases` to the
    // outer root `words` and cleared `b.moved = None` for every binding with
    // root `words`. Moving `words` itself (or a genuine alias `alias := words`)
    // in the same loop body was wiped at the back-edge and accepted with 0
    // diagnostics, racing under `go run -race`.
    let r_outer = check(
        "fn chunk_of(words []int) []int {\n\
           return append([]int{}, words...)\n\
         }\n\
         fn worker(s []int, done chan<- bool) {\n\
           for i in 0..100 {\n\
             s[0] = i\n\
           }\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           words := []int{1, 2, 3, 4}\n\
           done := make(chan bool, 4)\n\
           for i in 0..2 {\n\
             chunk := chunk_of(words)\n\
             _ = chunk\n\
             task worker(move words, done)\n\
           }\n\
         }",
    );
    assert_eq!(r_outer.verdict, Verdict::Reject, "{:?}", r_outer.diagnostics);
    assert_eq!(first_error_rule(&r_outer).as_deref(), Some("A1b"));
    assert!(
        r_outer.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 for outer slice moved in loop alongside fresh copy, got {:?}",
        r_outer.diagnostics
    );

    let r_alias = check(
        "fn chunk_of(words []int) []int {\n\
           return append([]int{}, words...)\n\
         }\n\
         fn worker(s []int, done chan<- bool) {\n\
           s[0] = 99\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           words := []int{1, 2, 3, 4}\n\
           done := make(chan bool, 4)\n\
           for i in 0..2 {\n\
             chunk := chunk_of(words)\n\
             _ = chunk\n\
             alias := words\n\
             task worker(move alias, done)\n\
           }\n\
         }",
    );
    assert_eq!(r_alias.verdict, Verdict::Reject, "{:?}", r_alias.diagnostics);
    assert_eq!(first_error_rule(&r_alias).as_deref(), Some("A1b"));
    assert!(
        r_alias.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 for alias moved in loop alongside fresh copy, got {:?}",
        r_alias.diagnostics
    );

    let r_shadow_append = check(
        "fn append(dst []int, src []int) []int {\n\
           _ = dst\n\
           return src\n\
         }\n\
         fn fake_copy(words []int) []int {\n\
           return append([]int{}, words)\n\
         }\n\
         fn worker(s []int, done chan<- bool) {\n\
           s[0] = 42\n\
           done <- true\n\
         }\n\
         fn main() {\n\
           words := []int{1, 2}\n\
           done := make(chan bool, 4)\n\
           for i in 0..2 {\n\
             chunk := fake_copy(words)\n\
             task worker(move chunk, done)\n\
           }\n\
         }",
    );
    assert_eq!(
        r_shadow_append.verdict,
        Verdict::Reject,
        "{:?}",
        r_shadow_append.diagnostics
    );
    assert!(r_shadow_append
        .diagnostics
        .iter()
        .any(|d| d.code == "HYB-E3003"));
}

