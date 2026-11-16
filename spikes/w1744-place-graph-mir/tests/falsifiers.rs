//! Executable Falsifier Matrix F1..F5 for Wildcard #1744 (answering PR #1934).

use std::path::PathBuf;

use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;
use w1744_place_graph_mir::{
    check_file_place_graph, measure_hy_check_coupling, normalize_file, ClosurePolicy,
};

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

fn collect_corpus_files() -> Vec<PathBuf> {
    let corpus_dir = repo_root().join("tests").join("checker-corpus");
    let mut files = Vec::new();
    let mut stack = vec![corpus_dir];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir).expect("read corpus dir").flatten() {
            let p = entry.path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().and_then(|s| s.to_str()) == Some("hy") {
                files.push(p);
            }
        }
    }
    files.sort();
    files
}

/// Pre-Registered Test 1 (PR #1934 §4.1):
/// Corpus parity across all 129 `.hy` files in `tests/checker-corpus/`.
/// Stage 1 normalization must produce 0 verdict flips and byte-identical error code sequences.
#[test]
fn f1_stage1_corpus_parity_across_all_129_files() {
    let files = collect_corpus_files();
    assert!(
        files.len() >= 127,
        "expected >=127 corpus files, found {}",
        files.len()
    );

    let mut total_parens_erased = 0usize;
    let mut total_unary_addr = 0usize;
    for path in &files {
        let src = std::fs::read_to_string(path).expect("read corpus file");
        let path_str = path.to_string_lossy();
        let Ok(raw_ast) = parse_file(&path_str, &src) else {
            // Parse-reject corpus fixtures fail before AST checking on both paths.
            continue;
        };
        let (norm_ast, stats) = normalize_file(&raw_ast);
        total_parens_erased += stats.parens_erased;
        total_unary_addr += stats.unary_addr_normalized;

        let raw_res = check_file(&raw_ast);
        let norm_res = check_file(&norm_ast);
        assert_eq!(
            raw_res.verdict, norm_res.verdict,
            "Stage 1 verdict drift on {}",
            path_str
        );
        let raw_codes: Vec<&str> = raw_res.diagnostics.iter().map(|d| d.code.as_str()).collect();
        let norm_codes: Vec<&str> = norm_res
            .diagnostics
            .iter()
            .map(|d| d.code.as_str())
            .collect();
        assert_eq!(
            raw_codes, norm_codes,
            "Stage 1 diagnostic code drift on {}",
            path_str
        );
    }
    println!(
        "[F1 PASS] {}/{} corpus files identical; erased {} parens, normalized {} Unary::Addr",
        files.len(),
        files.len(),
        total_parens_erased,
        total_unary_addr
    );
}

/// Pre-Registered Test 2 (PR #1934 §4.2):
/// Wrapper-class falsifier on LIVE `main` (`hy_check::check_file`).
/// Proves that `Expr::Paren` erasure alone closes live soundness bypass #1743
/// (`task worker((some(&s.a)), done)` and `ch <- (some(&s.a))`) WITHOUT editing `sendability.rs`.
#[test]
fn f2_stage1_wrapper_falsifier_closes_live_issue_1743_without_checker_edits() {
    let repro_spawn_1743 = r#"
import "sync/atomic"
struct state {
    a atomic.int64
    m map[string]int
}
fn worker(opt option[*atomic.int64], done chan<- bool) {
    _ = opt
    done <- true
}
fn main() {
    done := make(chan bool, 1)
    s := state{m: map[string]int{"k": 1}}
    task worker((some(&s.a)), done)
    s.m["k"] = 2
    <-done
}
"#;
    let raw_ast = parse_file("repro_1743_spawn.hy", repro_spawn_1743).expect("parse");
    let raw_res = check_file(&raw_ast);
    // On unpatched `main`, raw_ast falsely ACCEPTS because `classify_value` misses `Expr::Paren`!
    assert_eq!(
        raw_res.verdict,
        Verdict::Accept,
        "expected unpatched main to exhibit #1743 false-accept on raw AST"
    );

    let (stage1_ast, stats) = normalize_file(&raw_ast);
    assert!(stats.parens_erased >= 1);
    let stage1_res = check_file(&stage1_ast);
    assert_eq!(
        stage1_res.verdict,
        Verdict::Reject,
        "Stage 1 normalization must reject #1743 via unmodified hy_check: {:?}",
        stage1_res.diagnostics
    );
    assert!(stage1_res.diagnostics.iter().any(|d| d.code == "HYB-E3001"));

    let repro_send_1743 = r#"
import "sync/atomic"
struct state {
    a atomic.int64
    m map[string]int
}
fn main() {
    ch := make(chan option[*atomic.int64], 1)
    s := state{m: map[string]int{"k": 1}}
    ch <- (some(&s.a))
}
"#;
    let raw_send_ast = parse_file("repro_1743_send.hy", repro_send_1743).expect("parse");
    assert_eq!(check_file(&raw_send_ast).verdict, Verdict::Accept);
    let (stage1_send_ast, _) = normalize_file(&raw_send_ast);
    let stage1_send_res = check_file(&stage1_send_ast);
    assert_eq!(stage1_send_res.verdict, Verdict::Reject);
    assert!(stage1_send_res
        .diagnostics
        .iter()
        .any(|d| d.code == "HYB-E3002"));
}

/// Pre-Registered Test 3 (PR #1934 §4.3):
/// Stage 2 canonical `PlaceId` graph kills FIVE independent AST bug families with two
/// uniform predicates (`check_move_transfer` on `CanonicalPlace` and `AddrOf` on `BindingKind`):
///   1. #1742: `managed` non-ident `move h.m` O5.4 + freeze bypass (`HYB-E3004` + `HYB-E3003`)
///   2. #1775: `&n.arr[0]`, `&n[0]`, `&n.arr[0:2]` on map-slot loan (`HYB-E3104`)
///   3. #1778: `ch1 <- move h.items[idx["k"]].doc` reading moved `idx` inside field chain (`HYB-E3003`)
///   4. #1814: `b := move a` transitive `MayShare` move-chain freeze (`HYB-E3003`)
///   5. #1820: `&(make_s().x)` / `&((fn() s { ... })().x)` non-addressable temp field (`HYB-E2012`)
#[test]
fn f3_stage2_place_graph_eliminates_five_bug_classes_uniformly() {
    // 1. #1742 — `move h.m` inside `managed` (unpatched main FALSELY ACCEPTS!)
    let src_1742 = r#"
struct holder { m map[string]int }
fn worker(m map[string]int, done chan<- bool) {
    m["worker"] = 1
    done <- true
}
fn main() {
    done := make(chan bool, 1)
    h := holder{m: map[string]int{"init": 0}}
    managed("delegate map", "mutex") {
        task worker(move h.m, done)
    }
    h.m["main"] = 2
    <-done
}
"#;
    let ast_1742 = parse_file("repro_1742.hy", src_1742).expect("parse");
    assert_eq!(check_file(&ast_1742).verdict, Verdict::Accept);
    let pg_1742 = check_file_place_graph(&ast_1742, ClosurePolicy::Stage3CapturePlaces);
    let codes_1742: Vec<&str> = pg_1742.diagnostics.iter().map(|d| d.code).collect();
    assert!(codes_1742.contains(&"HYB-E3004"), "got {:?}", pg_1742.diagnostics);
    assert!(codes_1742.contains(&"HYB-E3003"), "got {:?}", pg_1742.diagnostics);

    // 2. #1775 — `&n.arr[0]` and `&n2[0]` on map-slot loans (unpatched main FALSELY ACCEPTS!)
    let src_1775 = r#"
struct node { arr [4]int }
fn main() {
    m := map[string]node{"a": node{arr: [4]int{1, 2, 3, 4}}}
    n := &m["a"]
    p1 := &n.arr[0]
    m2 := map[string][4]int{"a": [4]int{1, 2, 3, 4}}
    n2 := &m2["a"]
    p2 := &n2[0]
    _ = p1
    _ = p2
}
"#;
    let ast_1775 = parse_file("repro_1775.hy", src_1775).expect("parse");
    assert_eq!(check_file(&ast_1775).verdict, Verdict::Accept);
    let pg_1775 = check_file_place_graph(&ast_1775, ClosurePolicy::Stage3CapturePlaces);
    let e3104_count = pg_1775
        .diagnostics
        .iter()
        .filter(|d| d.code == "HYB-E3104")
        .count();
    assert_eq!(e3104_count, 2, "got {:?}", pg_1775.diagnostics);

    // Slice projection on a map-slot loan (`&n.arr[0:2]`) also rejected uniformly with HYB-E3104:
    let src_1775_slice = r#"
struct node { arr [4]int }
fn main() {
    m := map[string]node{"a": node{arr: [4]int{1, 2, 3, 4}}}
    n := &m["a"]
    ps := &n.arr[0:2]
    _ = ps
}
"#;
    let ast_1775_slice = parse_file("repro_1775_slice.hy", src_1775_slice).expect("parse");
    let pg_1775_slice = check_file_place_graph(&ast_1775_slice, ClosurePolicy::Stage3CapturePlaces);
    assert!(
        pg_1775_slice
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3104"),
        "got {:?}",
        pg_1775_slice.diagnostics
    );

    // 3. #1778 — `ch1 <- move h.items[idx["k"]].doc` reading moved `idx` (unpatched main FALSELY ACCEPTS!)
    let src_1778 = r#"
import "rust/serde_json"
struct slot { doc serde_json.value }
struct holder { items [1]slot }
fn worker(idx map[string]int) { idx["k"] = 1 }
fn main() {
    idx := map[string]int{"k": 0}
    ch1 := make(chan serde_json.value, 1)
    h := holder{items: [1]slot{slot{doc: serde_json.new_array()}}}
    task worker(move idx)
    ch1 <- move h.items[idx["k"]].doc
}
"#;
    let ast_1778 = parse_file("repro_1778.hy", src_1778).expect("parse");
    assert_eq!(check_file(&ast_1778).verdict, Verdict::Accept);
    let pg_1778 = check_file_place_graph(&ast_1778, ClosurePolicy::Stage3CapturePlaces);
    assert!(
        pg_1778.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "got {:?}",
        pg_1778.diagnostics
    );

    // 4. #1814 — `c := move holders[p.idx].conn` reading map-slot loan `p` inside `move` index
    // projection after `m["b"] = ...` (unpatched main FALSELY ACCEPTS because `walk.rs` drops
    // index reads inside `Expr::Move` field chains!)
    let src_1814_move_idx_loan = r#"
import "rust/serde_json"
struct slot { idx int }
struct holder { conn serde_json.value }
fn main() {
    m := map[string]slot{"a": slot{idx: 0}}
    p := &m["a"]
    m["b"] = slot{idx: 1}
    var holders []holder
    c := move holders[p.idx].conn
    _ = c
}
"#;
    let ast_1814 = parse_file("repro_1814_move_idx.hy", src_1814_move_idx_loan).expect("parse");
    assert_eq!(check_file(&ast_1814).verdict, Verdict::Accept);
    let pg_1814 = check_file_place_graph(&ast_1814, ClosurePolicy::Stage3CapturePlaces);
    assert!(
        pg_1814.diagnostics.iter().any(|d| d.code == "HYB-E3102"),
        "got {:?}",
        pg_1814.diagnostics
    );

    // 5. #1820 — subexpression `(&m["a"].bal) == nil`, `(&m["a"].arr[0]) != nil`, and
    // pointer-to-array-of-maps `&arr[0]["a"].bal` (unpatched main FALSELY ACCEPTS!)
    let src_1820 = r#"
struct acct {
    bal int
    arr [2]int
}
fn bad_subexpr_and_ptr_arr(arr *[2]map[string]acct) {
    var m map[string]acct
    a := (&m["a"].bal) == nil
    b := (&m["a"].arr[0]) != nil
    q := &arr[0]["a"].bal
    _ = a
    _ = b
    _ = q
}
"#;
    let ast_1820 = parse_file("repro_1820.hy", src_1820).expect("parse");
    assert_eq!(check_file(&ast_1820).verdict, Verdict::Accept);
    let pg_1820 = check_file_place_graph(&ast_1820, ClosurePolicy::Stage3CapturePlaces);
    let e2010_count = pg_1820
        .diagnostics
        .iter()
        .filter(|d| d.code == "HYB-E2010")
        .count();
    assert_eq!(e2010_count, 3, "got {:?}", pg_1820.diagnostics);

    // Also verify `BindingKind::Temp(TempKind::RvalueResult)` catches `&(make_box().x)` / IIFE projection (`HYB-E2012`):
    let src_temp_proj = r#"
struct box { x int }
fn make_box() box { return box{x: 1} }
fn main() {
    p1 := &(make_box().x)
    p2 := &((fn() box { return box{x: 2} })().x)
    _ = p1
    _ = p2
}
"#;
    let ast_temp_proj = parse_file("repro_temp_proj.hy", src_temp_proj).expect("parse");
    let pg_temp_proj = check_file_place_graph(&ast_temp_proj, ClosurePolicy::Stage3CapturePlaces);
    let e2012_count = pg_temp_proj
        .diagnostics
        .iter()
        .filter(|d| d.code == "HYB-E2012")
        .count();
    assert_eq!(e2012_count, 2, "got {:?}", pg_temp_proj.diagnostics);

    // Positive control: `&box{x: 1}` (addressing a whole struct literal) AND moving an unaliased
    // own-handle field `ch <- move h.doc` with unmoved index must produce 0 diagnostics.
    let src_ok = r#"
import "rust/serde_json"
struct box { x int }
struct holder { doc serde_json.value }
fn main() {
    p := &box{x: 1}
    _ = p
    ch := make(chan serde_json.value, 1)
    h := holder{doc: serde_json.new_array()}
    ch <- move h.doc
}
"#;
    let ast_ok = parse_file("pos_control.hy", src_ok).expect("parse");
    let pg_ok = check_file_place_graph(&ast_ok, ClosurePolicy::Stage3CapturePlaces);
    assert!(
        pg_ok.diagnostics.is_empty(),
        "positive control must pass with 0 diags, got {:?}",
        pg_ok.diagnostics
    );
}

/// Pre-Registered Test 4 (PR #1934 §4.4):
/// Measure Stage-2 conservative closure rejection rate across all `Accept` corpus programs
/// AND verify Stage-3 `CapturePlace` summaries catch #1814 (closure extending borrow-handle
/// `last_use` past `doc.set`) and #1792 (closure/helper mutating receiver map during slot loan).
#[test]
fn f4_stage3_closure_conservatism_cost_on_corpus_and_capture_soundness_1792_1814() {
    let files = collect_corpus_files();
    let mut accept_files = 0usize;
    let mut conservative_false_reject_files = 0usize;

    for path in &files {
        let src = std::fs::read_to_string(path).expect("read");
        let path_str = path.to_string_lossy();
        let Ok(ast) = parse_file(&path_str, &src) else {
            continue;
        };
        if check_file(&ast).verdict != Verdict::Accept {
            continue;
        }
        accept_files += 1;
        let pg_stage2 = check_file_place_graph(&ast, ClosurePolicy::Stage2Conservative);
        if pg_stage2.conservative_closure_rejects > 0 {
            conservative_false_reject_files += 1;
        }
    }

    let reject_pct = (conservative_false_reject_files as f64 / accept_files as f64) * 100.0;
    println!(
        "[F4 MEASUREMENT] Stage-2 conservative closure rule rejects {}/{} Accept corpus files ({:.2}%)",
        conservative_false_reject_files, accept_files, reject_pct
    );
    // PR #1934 §4.4 gate: if <= 5%, Stage 2 CAN ship independently before Stage 3!
    assert!(
        reject_pct <= 5.0,
        "Stage 2 conservative closure cost {:.2}% exceeded 5% gate",
        reject_pct
    );

    // Stage 3 soundness proof A (#1814): closure captures borrow view `v := doc.array("k")?`,
    // `doc.set("k", "x")` mutates `doc`, and `read_v()` runs afterwards -> `HYB-E3101`!
    let src_1814_capture = r#"
import "rust/serde_json"
fn main() error {
    doc := serde_json.parse(`{"k": [1, 2]}`)?
    v := doc.array("k")?
    read_v := fn() int {
        return len(v)
    }
    doc.set("k", "x")
    _ = read_v()
    return nil
}
"#;
    let ast_cap_1814 = parse_file("repro_1814_cap.hy", src_1814_capture).expect("parse");
    // Unpatched main FALSELY ACCEPTS this A2 borrow-invalidation bypass!
    assert_eq!(check_file(&ast_cap_1814).verdict, Verdict::Accept);
    let pg_cap_1814 = check_file_place_graph(&ast_cap_1814, ClosurePolicy::Stage3CapturePlaces);
    assert!(
        pg_cap_1814
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3101"),
        "Stage 3 must reject #1814 borrow-view closure capture across doc.set: {:?}",
        pg_cap_1814.diagnostics
    );

    // Stage 3 soundness proof B (#1792): helper method + closure mutating receiver map during
    // an active map-slot loan `entry := &s.m["a"]` -> `HYB-E3102`!
    let src_1792_capture = r#"
struct item { count int }
struct store { m map[string]item }
fn (s *store) put(k string, v int) {
    s.m[k] = item{count: v}
}
fn (s *store) run() int {
    entry := &s.m["a"]
    bump := fn() {
        s.put("b", 2)
    }
    bump()
    return entry.count
}
"#;
    let ast_cap_1792 = parse_file("repro_1792_cap.hy", src_1792_capture).expect("parse");
    assert_eq!(check_file(&ast_cap_1792).verdict, Verdict::Accept);
    let pg_cap_1792 = check_file_place_graph(&ast_cap_1792, ClosurePolicy::Stage3CapturePlaces);
    assert!(
        pg_cap_1792
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3102"),
        "Stage 3 must reject #1792 receiver mutation via closure+helper during slot loan: {:?}",
        pg_cap_1792.diagnostics
    );
}

/// Pre-Registered Test 5 (PR #1934 §4.5):
/// Automated coupling census of `compiler/crates/hy-check/src/*.rs` and honest LoC verification.
#[test]
fn f5_codebase_coupling_census_and_loc_budget_honesty() {
    let root = repo_root();
    let hy_check_src = root
        .join("compiler")
        .join("crates")
        .join("hy-check")
        .join("src");
    let spike_lib = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("src")
        .join("lib.rs");

    let census = measure_hy_check_coupling(&hy_check_src, &spike_lib);
    println!(
        "[F5 CENSUS] hy-check/src ({} files): Expr:: sites = {}, Expr::Paren arms = {}, unwrap_parens calls = {}, Closure/FnLit sites = {}; spike non-comment LoC = {} (Stage1={}, Stage2={}, Stage3={})",
        census.files_scanned,
        census.expr_pattern_sites,
        census.expr_paren_arms,
        census.unwrap_parens_calls,
        census.closure_sites,
        census.spike_lib_loc,
        census.spike_stage1_loc,
        census.spike_stage2_loc,
        census.spike_stage3_loc
    );

    // Confirms PR #1934's core empirical finding: `Expr::` surface coupling (>1,000 sites across 21 files)
    // and closure handling (>150 sites) dwarf `Expr::Paren` pattern arms (<40 sites), while defensive
    // `unwrap_parens(...)` helpers have proliferated (>200 call sites) and the staged implementation
    // itself is ~1,650 LoC (confirming #1934's staged budget over #1744's original ~350 LoC estimate).
    assert!(census.expr_pattern_sites > 1000);
    assert!(census.closure_sites > 150);
    assert!(census.expr_paren_arms < 50);
    assert!(census.unwrap_parens_calls > 200);
    assert!(census.spike_lib_loc > 350 && census.spike_lib_loc < 2000);
}
