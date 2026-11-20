//! Conformance and corpus-audit suite for Checked Allocation (#4492).
//!
//! Covers:
//!   1. Summary lattice algebra (`Const`, `Linear`, `Unbounded`)
//!   2. Zero-allocation (`Const(0)`) certification
//!   3. Constant-allocation (`Const(k)`) certification + N5 zero-value map accounting
//!   4. Zero-annotation `Linear` certification over slice/map/string/int parameters
//!   5. Soundness guards against mutated and shadowed parameters
//!   6. Fail-closed `HYB-E6008` rejection + fix-its on unbounded loops, channels, recursion, and foreign calls
//!   7. Fail-closed `HYB-E6007` rejection + fix-its on finite budget exceedances
//!   8. Interprocedural call, method, closure, and `task` spawn composition
//!   9. `managed("reason", "category")` hatch accounting (checked vs total bounds)
//!  10. Source `@heap-budget:` pragmas + backwards-compatibility of plain `check_file`
//!  11. Static audit across all 47 ACCEPT and ACCEPT-MANAGED corpus programs

use std::collections::BTreeMap;
use std::path::Path;

use hy_check::{
    audit_corpus_dir, check_file, check_file_with_alloc, parse_budget_spec, parse_cli_budget_arg,
    AllocCheckConfig, AllocSummary, CheckResult, HeapBudget, Verdict,
};

fn parse_and_check_alloc(src: &str, cfg: &AllocCheckConfig) -> CheckResult {
    let ast = hy_syntax::parse_file("alloc_test.hy", src).expect("test program must parse");
    check_file_with_alloc(&ast, Some(src), cfg)
}

fn plain_check(src: &str) -> CheckResult {
    let ast = hy_syntax::parse_file("alloc_test.hy", src).expect("test program must parse");
    check_file(&ast)
}

fn error_codes(res: &CheckResult) -> Vec<String> {
    res.diagnostics
        .iter()
        .filter(|d| d.severity == hy_diag::Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

#[test]
fn t01_summary_lattice_laws_and_budget_satisfaction() {
    let c0 = AllocSummary::Const(0);
    let c3 = AllocSummary::Const(3);
    let c5 = AllocSummary::Const(5);
    let lin_x = AllocSummary::linear("xs", 2, 1);
    let lin_x2 = AllocSummary::linear("xs", 4, 0);
    let lin_y = AllocSummary::linear("ys", 1, 0);
    let unb = AllocSummary::Unbounded;

    // Sequential composition
    assert_eq!(c0.seq(&c3), AllocSummary::Const(3));
    assert_eq!(c3.seq(&c5), AllocSummary::Const(8));
    assert_eq!(c3.seq(&lin_x), AllocSummary::linear("xs", 2, 4));
    assert_eq!(lin_x.seq(&lin_x2), AllocSummary::linear("xs", 6, 1));
    assert_eq!(lin_x.seq(&lin_y), AllocSummary::Unbounded);
    assert_eq!(c3.seq(&unb), AllocSummary::Unbounded);

    // Branch join
    assert_eq!(c3.join(&c5), AllocSummary::Const(5));
    assert_eq!(c5.join(&lin_x), AllocSummary::linear("xs", 2, 5));
    assert_eq!(lin_x.join(&lin_x2), AllocSummary::linear("xs", 4, 1));
    assert_eq!(lin_x.join(&lin_y), AllocSummary::Unbounded);

    // Loop amplification
    assert_eq!(c0.amplify_const(1000), AllocSummary::Const(0));
    assert_eq!(c0.amplify_param("n", 0), AllocSummary::Const(0));
    assert_eq!(c3.amplify_const(4), AllocSummary::Const(12));
    assert_eq!(lin_x.amplify_const(3), AllocSummary::linear("xs", 6, 3));
    assert_eq!(c3.amplify_param("n", 0), AllocSummary::linear("n", 3, 0));
    assert_eq!(c3.amplify_param("n", 1), AllocSummary::linear("n", 3, 3));
    assert_eq!(lin_x.amplify_param("n", 0), AllocSummary::Unbounded);

    // Budget satisfaction
    assert!(c3.satisfies(&HeapBudget::Const(3)));
    assert!(c3.satisfies(&HeapBudget::Const(10)));
    assert!(!c5.satisfies(&HeapBudget::Const(3)));
    assert!(!lin_x.satisfies(&HeapBudget::Const(100)));
    assert!(lin_x.satisfies(&HeapBudget::Bounded));
    assert!(lin_x.satisfies(&parse_budget_spec("linear(xs)").unwrap()));
    assert!(lin_x.satisfies(&parse_budget_spec("linear(xs, 2, 1)").unwrap()));
    assert!(!lin_x.satisfies(&parse_budget_spec("linear(xs, 1, 5)").unwrap()));
    assert!(!lin_x.satisfies(&parse_budget_spec("linear(other)").unwrap()));
    assert!(!unb.satisfies(&HeapBudget::Bounded));

    let (target, b) = parse_cli_budget_arg("tally=linear:shard:1+1").unwrap();
    assert_eq!(target.as_deref(), Some("tally"));
    assert!(AllocSummary::linear("shard", 1, 1).satisfies(&b));
}

#[test]
fn t02_zero_allocation_certification() {
    let src = r#"
import "sync"
import "sync/atomic"

type node_id = int

struct stats {
    mu sync.mutex
    count int
}

fn compute(a int, b int, hits *atomic.int64, st *stats) int {
    hits.add(1)
    st.mu.lock()
    st.count += a
    st.mu.unlock()
    o := some(b)
    var extra int
    if o.is_some() {
        extra = int(u8(200)) + int(node_id(3))
    }
    for i in 0..a {
        extra += i
    }
    return st.count + extra + int(hits.load())
}
"#;
    let cfg = AllocCheckConfig {
        program_budget: Some(HeapBudget::Const(0)),
        ..Default::default()
    };
    let res = parse_and_check_alloc(src, &cfg);
    assert_eq!(res.verdict, Verdict::Accept, "diags: {:?}", res.diagnostics);
    assert_eq!(res.alloc.total_sites, 0);
    assert_eq!(
        res.alloc.functions["compute"].checked_summary,
        AllocSummary::Const(0)
    );
}

#[test]
fn t03_constant_allocation_and_map_slot_loan_precision() {
    let src = r#"
const batch_len = 4
const banner = "hello " + "world"

struct item {
    id int
}

fn fixed_work() int {
    m := map[string]item{"a": {id: 1}}
    slot := &m["a"]
    slot.id = 9
    xs := make([]int, 3, 10)
    xs[0] = len(banner)
    var out []int
    for i in 0..batch_len {
        out = append(out, i + slot.id)
    }
    p := new(item)
    p.id = xs[0]
    return len(out) + p.id
}
"#;
    // m := map{1} (1), xs := make (1), 4 * append (4), new(item) (1) = Const(7);
    // `&m["a"]` is an O8 map-slot loan (stack copy + store-back -> 0 heap allocs),
    // and `"hello " + "world"` is a compile-time string fold (0 heap allocs).
    let ok_cfg = AllocCheckConfig {
        program_budget: Some(HeapBudget::Const(7)),
        ..Default::default()
    };
    let ok = parse_and_check_alloc(src, &ok_cfg);
    assert_eq!(ok.verdict, Verdict::Accept, "diags: {:?}", ok.diagnostics);
    assert_eq!(
        ok.alloc.functions["fixed_work"].checked_summary,
        AllocSummary::Const(7)
    );

    let tight_cfg = AllocCheckConfig {
        program_budget: Some(HeapBudget::Const(6)),
        ..Default::default()
    };
    let fail = parse_and_check_alloc(src, &tight_cfg);
    assert_eq!(fail.verdict, Verdict::Reject);
    assert_eq!(error_codes(&fail), vec!["HYB-E6007"]);
    assert!(!fail.diagnostics[0].fixits.is_empty());
}

#[test]
fn t04_n5_zero_map_initialization_counted_faithfully() {
    let src = r#"
struct bag {
    tag string
    counts map[string]int
}

fn init_bags() int {
    var m map[string]int
    var two [2]bag
    partial := bag{tag: "p"}
    made := make([]bag, 3)
    return len(m) + len(two[0].counts) + len(partial.counts) + len(made)
}
"#;
    let res = plain_check(src);
    assert_eq!(res.verdict, Verdict::Accept, "diags: {:?}", res.diagnostics);
    // m (1) + two [2]bag (2) + partial omitted counts (1) + make([]bag, 3) (1 + 3*1 = 4) = Const(8)
    assert_eq!(
        res.alloc.functions["init_bags"].checked_summary,
        AllocSummary::Const(8)
    );
}

#[test]
fn t05_linear_bounds_certified_without_annotations() {
    let src = r#"
fn tally_words(shard []string) map[string]int {
    counts := map[string]int{}
    for w in shard {
        counts[w]++
    }
    return counts
}

fn build_seq(n int) []int {
    buf := make([]int, 0, n)
    for i in 0..n {
        buf = append(buf, i)
    }
    return buf
}

fn inclusive_seq(n int) []int {
    var out []int
    for i in 0..=n {
        out = append(out, i)
    }
    return out
}

fn copy_window(words []string, lo int, hi int) []string {
    return append([]string{}, words[lo:hi]...)
}

fn main() {
    a := tally_words(["go", "rust"])
    b := build_seq(4)
    c := inclusive_seq(2)
    d := copy_window(["x", "y", "z"], 0, 2)
    _ = len(a) + len(b) + len(c) + len(d)
}
"#;
    let cfg = AllocCheckConfig {
        require_bounded_all_fns: true,
        ..Default::default()
    };
    let res = parse_and_check_alloc(src, &cfg);
    assert_eq!(res.verdict, Verdict::Accept, "diags: {:?}", res.diagnostics);

    assert_eq!(
        res.alloc.functions["tally_words"].checked_summary,
        AllocSummary::linear("shard", 1, 1)
    );
    assert_eq!(
        res.alloc.functions["build_seq"].checked_summary,
        AllocSummary::linear("n", 2, 1)
    );
    assert_eq!(
        res.alloc.functions["inclusive_seq"].checked_summary,
        AllocSummary::linear("n", 1, 1)
    );
    assert_eq!(
        res.alloc.functions["copy_window"].checked_summary,
        AllocSummary::linear("words", 1, 1)
    );
    // In `main`, every call passes constant literals, so interprocedural substitution
    // specializes every linear callee summary to a concrete `Const`!
    assert!(
        res.alloc.functions["main"].checked_summary.is_const(),
        "expected Const for main, got {}",
        res.alloc.functions["main"].checked_summary
    );
}

#[test]
fn t06_mutated_or_shadowed_parameter_widens_fail_closed() {
    let src = r#"
fn mutated_bound(n int) []int {
    n = n * 100
    var xs []int
    for i in 0..n {
        xs = append(xs, i)
    }
    return xs
}

fn shadowed_bound(n int, flag bool) []int {
    var xs []int
    if flag {
        n := flag
        _ = n
    }
    return xs
}
"#;
    let mut fn_budgets = BTreeMap::new();
    fn_budgets.insert(
        "mutated_bound".to_string(),
        parse_budget_spec("linear(n)").unwrap(),
    );
    let cfg = AllocCheckConfig {
        fn_budgets,
        ..Default::default()
    };
    let res = parse_and_check_alloc(src, &cfg);
    assert_eq!(res.verdict, Verdict::Reject);
    assert_eq!(error_codes(&res), vec!["HYB-E6008"]);
    assert_eq!(
        res.alloc.functions["mutated_bound"].checked_summary,
        AllocSummary::Unbounded
    );
    assert_eq!(
        res.alloc.functions["shadowed_bound"].checked_summary,
        AllocSummary::Const(0)
    );
}

#[test]
fn t07_unbounded_loops_channels_and_foreign_calls_reject_e6008_with_fixits() {
    let cases = [
        // Condition-controlled loop allocating per step
        r#"
fn while_grow(limit int) []int {
    var xs []int
    i := 0
    for i < limit {
        xs = append(xs, i)
        i++
    }
    return xs
}
"#,
        // Channel stream range allocating per message
        r#"
fn chan_concat(ch <-chan string) string {
    acc := ""
    for msg in ch {
        acc = acc + msg
    }
    return acc
}
"#,
        // Two distinct linear parameters combined
        r#"
fn two_params(a int, b int) int {
    xs := make([]int, a)
    ys := make([]int, b)
    return len(xs) + len(ys)
}
"#,
        // Quadratic nested parameter loop
        r#"
fn quadratic(n int) []int {
    var xs []int
    for i in 0..n {
        for j in 0..n {
            xs = append(xs, i + j)
        }
    }
    return xs
}
"#,
        // Unmodeled foreign I/O call
        r#"
import "os"
fn load_bytes(p string) ([]byte, error) {
    data := os.read_file(p)?
    return data, nil
}
"#,
    ];

    let cfg = AllocCheckConfig {
        require_bounded_all_fns: true,
        ..Default::default()
    };
    for (idx, src) in cases.iter().enumerate() {
        // Plain check (no budget flags) stays 100% backwards-compatible!
        let plain = plain_check(src);
        assert_eq!(
            plain.verdict,
            Verdict::Accept,
            "case {idx} must pass un-budgeted check: {:?}",
            plain.diagnostics
        );

        let checked = parse_and_check_alloc(src, &cfg);
        assert_eq!(
            checked.verdict,
            Verdict::Reject,
            "case {idx} must reject under --check-alloc"
        );
        assert_eq!(error_codes(&checked), vec!["HYB-E6008"], "case {idx}");
        assert!(
            !checked.diagnostics[0].fixits.is_empty(),
            "case {idx} HYB-E6008 must carry teaching fix-its"
        );
    }
}

#[test]
fn t08_interprocedural_spawn_closure_and_recursion_composition() {
    // 1. Pure recursion stays Const(0); allocating recursion widens to Unbounded.
    let rec_src = r#"
fn fib(n int) int {
    if n <= 1 {
        return n
    }
    return fib(n - 1) + fib(n - 2)
}

fn alloc_tree(depth int) []int {
    if depth <= 0 {
        return []int{1}
    }
    left := alloc_tree(depth - 1)
    return append(left, depth)
}
"#;
    let plain = plain_check(rec_src);
    assert_eq!(plain.verdict, Verdict::Accept);
    assert_eq!(
        plain.alloc.functions["fib"].checked_summary,
        AllocSummary::Const(0)
    );
    assert_eq!(
        plain.alloc.functions["alloc_tree"].checked_summary,
        AllocSummary::Unbounded
    );

    // 2. Task spawn + parameter forwarding + local closure call composition.
    let spawn_src = r#"
fn worker(count int, out chan<- int) {
    buf := make([]int, count)
    out <- len(buf)
}

fn dispatch(batch_len int) int {
    ch := make(chan int, 1)
    scale := 2
    helper := fn(k int) int {
        tmp := []int{k, scale}
        return len(tmp)
    }
    extra := helper(3)
    task worker(batch_len, ch)
    return (<-ch) + extra
}
"#;
    let cfg = AllocCheckConfig {
        require_bounded_all_fns: true,
        ..Default::default()
    };
    let res = parse_and_check_alloc(spawn_src, &cfg);
    assert_eq!(res.verdict, Verdict::Accept, "diags: {:?}", res.diagnostics);
    assert_eq!(
        res.alloc.functions["worker"].checked_summary,
        AllocSummary::linear("count", 1, 1)
    );
    // dispatch: make(chan) (1) + helper capture (1) + helper(3) SliceLit (1) + task spawn (1)
    //           + worker(batch_len) (1 * batch_len + 1) = linear(1 * batch_len + 5)
    assert_eq!(
        res.alloc.functions["dispatch"].checked_summary,
        AllocSummary::linear("batch_len", 1, 5)
    );
}

#[test]
fn t09_managed_hatch_exempts_checked_bound_while_auditing_total() {
    let src = r#"
fn ingest_with_hatch(ch <-chan int) int {
    total := 0
    managed("bounded by upstream rate limiter", "other", "ALLOC-01") {
        var log []int
        for v in ch {
            log = append(log, v)
        }
        total = len(log)
    }
    return total
}
"#;
    let cfg = AllocCheckConfig {
        program_budget: Some(HeapBudget::Const(0)),
        ..Default::default()
    };
    let res = parse_and_check_alloc(src, &cfg);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "managed block must discharge checked-subset budget obligation: {:?}",
        res.diagnostics
    );
    let rep = &res.alloc.functions["ingest_with_hatch"];
    assert_eq!(rep.checked_summary, AllocSummary::Const(0));
    assert_eq!(rep.total_summary, AllocSummary::Unbounded);
    assert_eq!(rep.checked_sites, 0);
    assert_eq!(rep.managed_sites, 1);
}

#[test]
fn t10_source_pragma_annotations_enforced_when_enabled() {
    let ok_src = r#"
// @heap-budget: const(0)
fn pure_add(a int, b int) int {
    return a + b
}

// @heap-budget: linear(xs, 1, 1)
fn count_items(xs []string) map[string]int {
    m := map[string]int{}
    for x in xs {
        m[x]++
    }
    return m
}
"#;
    let cfg = AllocCheckConfig {
        honor_source_pragmas: true,
        ..Default::default()
    };
    let ok = parse_and_check_alloc(ok_src, &cfg);
    assert_eq!(ok.verdict, Verdict::Accept, "diags: {:?}", ok.diagnostics);

    let bad_src = r#"
// @heap-budget: const(0)
fn accidental_alloc() []int {
    return []int{1, 2, 3}
}
"#;
    // Un-budgeted `check_file` stays permissive unless budget checking is opted in.
    assert_eq!(plain_check(bad_src).verdict, Verdict::Accept);

    let bad = parse_and_check_alloc(bad_src, &cfg);
    assert_eq!(bad.verdict, Verdict::Reject);
    assert_eq!(error_codes(&bad), vec!["HYB-E6007"]);
}

#[test]
fn t11_corpus_accept_side_static_audit_and_usefulness_gate() {
    let corpus_dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../tests/checker-corpus");
    let audit = audit_corpus_dir(&corpus_dir).expect("corpus static audit must succeed");

    // Exact 47 accept-side fixtures on main (45 ACCEPT + 2 ACCEPT-MANAGED).
    assert_eq!(audit.accept_programs_total, 47);
    assert_eq!(audit.accept_clean_count, 45);
    assert_eq!(audit.accept_managed_count, 2);

    // Issue #4492 M2(b) usefulness gates:
    //   1. Certifies with 0 annotations at least half of ACCEPT-side programs
    //      that use no data-dependent construct.
    assert!(
        audit.non_data_dependent_programs > 0,
        "expected non-data-dependent accept programs"
    );
    assert!(
        audit.non_data_dependent_certified_const * 2 >= audit.non_data_dependent_programs,
        "usefulness gate failed: certified {}/{} non-data-dependent programs as Const",
        audit.non_data_dependent_certified_const,
        audit.non_data_dependent_programs
    );

    //   2. Certifies with 0 annotations at least one genuinely Linear function
    //      in the shipped corpus (e.g. `15-fanin-per-task-state.hy` tally/chunk_of).
    assert!(
        audit.linear_functions >= 2,
        "expected >= 2 genuinely linear functions in shipped corpus, got {}",
        audit.linear_functions
    );
    let corpus15 = audit
        .rows
        .iter()
        .find(|r| r.file == "15-fanin-per-task-state.hy")
        .expect("corpus 15 present");
    assert!(
        corpus15
            .linear_functions
            .iter()
            .any(|(name, sum)| name == "tally" && *sum == AllocSummary::linear("shard", 1, 1)),
        "expected `tally: linear(1 * shard + 1)` in corpus 15, got {:?}",
        corpus15.linear_functions
    );

    let md = audit.render_markdown_summary();
    assert!(md.contains("Corpus Static Allocation Audit"));
    let dbl_hyphen = format!(" {}{} ", '-', '-');
    assert!(!md.contains('\u{2014}') && !md.contains('\u{2013}') && !md.contains(&dbl_hyphen));
}

#[test]
fn t12_cyclic_const_aliases_terminate_cleanly() {
    let src = r#"
const cyc_a = cyc_b
const cyc_b = cyc_a

fn probe() {
    _ = len(cyc_a)
    _ = cyc_a + "tail"
}
"#;
    let ast = hy_syntax::parse_file("cyc.hy", src).expect("parse ok");
    let res = check_file_with_alloc(
        &ast,
        Some(src),
        &AllocCheckConfig {
            require_bounded_all_fns: true,
            ..Default::default()
        },
    );
    assert!(
        !res.alloc.functions.is_empty(),
        "allocation pass must terminate on cyclic const aliases"
    );
}
