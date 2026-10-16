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
