//! hy-emit tests — golden-shape assertions over the emitted Go.
//!
//! Discipline: the two MVP-0 emitter bugs recovered from the crashed session
//! have named regression tests (bug-1: emission order; bug-2: make type).
//! Full end-to-end verification (go build + run + expected.txt diff) lives in
//! tools/run_examples.sh — these tests pin the emission SHAPES.

use hy_syntax::parse_file;

use super::emit_go;

fn emit(src: &str) -> String {
    let file = parse_file("t.hy", src).expect("parse");
    emit_go(&file).expect("emit")
}

fn emit_err(src: &str) -> String {
    let file = parse_file("t.hy", src).expect("parse");
    match emit_go(&file) {
        Ok(g) => panic!("expected unsupported error, got:\n{}", g),
        Err(e) => e.message,
    }
}

// ---------------------------------------------------------------------------
// The two recovered MVP-0 bugs — named regression tests
// ---------------------------------------------------------------------------

#[test]
fn bug1_struct_first_file_emits_imports_before_types() {
    // Struct declared FIRST (like example 03) — Go requires imports
    // immediately after `package`; the old single-pass emitter wrote `type`
    // decls before the import block.
    let go = emit("struct job {\n    input int\n}\n\nfn main() {\n    fmt.println(\"hi\")\n}\n");
    let pkg = go.find("package main").unwrap();
    let imp = go.find("import (").unwrap();
    let ty = go.find("type job struct").unwrap();
    assert!(
        pkg < imp && imp < ty,
        "order must be package < import < type:\n{}",
        go
    );
    assert!(go.contains("\"fmt\""), "fmt imported:\n{}", go);
}

#[test]
fn bug2_make_chan_renders_bare_type_not_value_literal() {
    // `make(chan job, 16)` — the parser encodes the type argument as an empty
    // StructLit; rendering it as `job{}` inside make() is invalid Go.
    let go =
        emit("struct job {\n    input int\n}\n\nfn main() {\n    jobs := make(chan job, 16)\n}\n");
    assert!(
        go.contains("make(chan job, 16)"),
        "bare type in make:\n{}",
        go
    );
    assert!(
        !go.contains("make(chan job{}, 16)"),
        "value literal leaked:\n{}",
        go
    );
}

#[test]
fn bug2_make_slice_types() {
    let go = emit("fn main() {\n    buf := make([]byte, 64 * 1024)\n}");
    assert!(go.contains("make([]byte, 64 * 1024)"), ":\n{}", go);
}

#[test]
fn map_literal_zero_value_is_direct_composite() {
    // The corpus spells empty maps as `map[string]int{}` (the parser surface
    // has no `make(map…)` encoding — maps are composite literals).
    let go = emit("fn main() {\n    m := map[string]int{}\n}");
    assert!(go.contains("m := map[string]int{}"), ":\n{}", go);
}

// ---------------------------------------------------------------------------
// Example 01 — hello (interpolation, printf, range with index)
// ---------------------------------------------------------------------------

#[test]
fn example01_golden_shape() {
    let src = r#"
fn main() {
    name := "Hybrid"
    count := 0
    count += 1
    fmt.println("hello, {name}!")
    fmt.printf("count=%d ok=%v\n", count, count == 1)
    let langs = ["Go", "Rust", "Hybrid"]
    for i, l in langs {
        fmt.println("{i}: {l}")
    }
}
"#;
    let go = emit(src);
    assert!(go.contains("name := \"Hybrid\""), ":\n{}", go);
    assert!(go.contains("count += 1"), ":\n{}", go);
    assert!(
        go.contains("fmt.Println(fmt.Sprintf(\"hello, %v!\", name))"),
        "interp → Sprintf:\n{}",
        go
    );
    assert!(
        go.contains("fmt.Printf(\"count=%d ok=%v\\n\", count, count == 1)"),
        "printf passthrough:\n{}",
        go
    );
    assert!(
        go.contains("langs := []string{\"Go\", \"Rust\", \"Hybrid\"}"),
        "inferred string slice:\n{}",
        go
    );
    assert!(
        go.contains("for i, l := range langs {"),
        "2-var range:\n{}",
        go
    );
    assert!(
        go.contains("fmt.Println(fmt.Sprintf(\"%v: %v\", i, l))"),
        ":\n{}",
        go
    );
}

// ---------------------------------------------------------------------------
// Example 02 — cli-cat (wrapper, ?-desugar, defer, byte hint, eprintln)
// ---------------------------------------------------------------------------

fn example02_src() -> String {
    r#"
fn main() (code int) {
    args := os.args()
    if len(args) < 2 {
        fmt.eprintln("usage: cat <file>")
        return 1
    }
    f := os.open(args[1])?
    defer f.close()
    data := io.read_all(f)?
    fmt.print(data)
    return 0
}
"#
    .to_string()
}

#[test]
fn example02_wrapper_shape_and_exit_contract() {
    let go = emit(&example02_src());
    assert!(go.contains("func main() {"), "wrapper main:\n{}", go);
    assert!(
        go.contains("func() (code int, hyErr error) {"),
        "closure carries named results + hidden error:\n{}",
        go
    );
    assert!(
        go.contains("code, hyErr := func()"),
        "receiver vars:\n{}",
        go
    );
    assert!(
        go.contains("fmt.Fprintln(os.Stderr, filepath.Base(os.Args[0])+\":\", hyErr)"),
        "E6 stderr line:\n{}",
        go
    );
    assert!(go.contains("os.Exit(1)"), ":\n{}", go);
    assert!(go.contains("os.Exit(code)"), ":\n{}", go);
}

#[test]
fn example02_try_desugar_and_defer() {
    let go = emit(&example02_src());
    assert!(
        go.contains("args := os.Args"),
        "os.args() → os.Args, parens dropped:\n{}",
        go
    );
    assert!(
        go.contains("f, hyErr0 := os.Open(args[1])"),
        "?-desugar binds temp error:\n{}",
        go
    );
    assert!(
        go.contains("hyErr = hyErr0") && go.contains("return"),
        "error path assigns + returns:\n{}",
        go
    );
    assert!(
        go.contains("defer f.Close()"),
        "foreign method normalized:\n{}",
        go
    );
    assert!(
        go.contains("data, hyErr1 := io.ReadAll(f)"),
        "second desugar, unique temp:\n{}",
        go
    );
    assert!(
        go.contains("os.Stdout.Write(data)"),
        "byte-binding hint routes print to Write:\n{}",
        go
    );
    assert!(
        go.contains("return 1, nil") && go.contains("return 0, nil"),
        ":\n{}",
        go
    );
}

// ---------------------------------------------------------------------------
// Example 03 — worker pool (structs-first, task→go, send/receive, close)
// ---------------------------------------------------------------------------

fn example03_src() -> String {
    r#"
struct job {
    input   int
    cost_ms int
}

struct result {
    worker int
    value  int
}

fn worker(id int, jobs <-chan job, results chan<- result) {
    for j in jobs {
        time.sleep(time.milliseconds(j.cost_ms))
        results <- result{worker: id, value: j.input * 2}
    }
}

fn main() {
    const n_workers = 4
    jobs := make(chan job, 16)
    results := make(chan result, 16)
    for i in 0..n_workers {
        task worker(i, jobs, results)
    }
    inputs := [3, 1, 4, 1, 5, 9, 2, 6]
    for _, v in inputs {
        jobs <- job{input: v, cost_ms: (v % 3)}
    }
    close(jobs)
    var got []int
    for len(got) < len(inputs) {
        r := <-results
        got = append(got, r.value)
    }
    sort.ints(got)
    fmt.println(got)
}
"#
    .to_string()
}

#[test]
fn example03_golden_shape() {
    let go = emit(&example03_src());
    let pkg = go.find("package main").unwrap();
    let imp = go.find("import (").unwrap();
    let ty = go.find("type job struct").unwrap();
    assert!(pkg < imp && imp < ty, "bug-1 shape holds for 03:\n{}", go);
    for p in ["\"fmt\"", "\"sort\"", "\"time\""] {
        assert!(go.contains(p), "auto-import {}:\n{}", p, go);
    }
    assert!(
        go.contains("func worker(id int, jobs <-chan job, results chan<- result) {"),
        ":\n{}",
        go
    );
    assert!(
        go.contains("for j := range jobs {"),
        "1-var range over chan:\n{}",
        go
    );
    assert!(
        go.contains("time.Sleep(time.Duration(j.cost_ms) * time.Millisecond)"),
        "duration constructor:\n{}",
        go
    );
    assert!(
        go.contains("results <- result{worker: id, value: j.input * 2}"),
        "struct literal send:\n{}",
        go
    );
    assert!(
        go.contains("for i := 0; i < n_workers; i++ {"),
        "range-for:\n{}",
        go
    );
    assert!(
        go.contains("go worker(i, jobs, results)"),
        "task → go:\n{}",
        go
    );
    assert!(
        go.contains("inputs := []int{3, 1, 4, 1, 5, 9, 2, 6}"),
        "inferred int slice:\n{}",
        go
    );
    assert!(
        go.contains("for _, v := range inputs {"),
        "underscore range:\n{}",
        go
    );
    assert!(
        go.contains("jobs <- job{input: v, cost_ms: (v % 3)}"),
        ":\n{}",
        go
    );
    assert!(go.contains("close(jobs)"), ":\n{}", go);
    assert!(go.contains("var got []int"), ":\n{}", go);
    assert!(go.contains("r := <-results"), "receive-define:\n{}", go);
    assert!(go.contains("got = append(got, r.value)"), ":\n{}", go);
    assert!(go.contains("sort.Ints(got)"), ":\n{}", go);
    assert!(go.contains("fmt.Println(got)"), ":\n{}", go);
    assert!(
        go.contains("const n_workers = 4"),
        "fn-local const:\n{}",
        go
    );
}

// ---------------------------------------------------------------------------
// Emission mechanics
// ---------------------------------------------------------------------------

#[test]
fn imports_are_sorted_and_deduped() {
    let go = emit("fn main() {\n    sort.ints(nil)\n    time.sleep(0)\n    fmt.println(\"x\")\n}");
    let imp = go.find("import (").unwrap();
    let end = go.find(")").unwrap();
    let block = &go[imp..end];
    let a = block.find("\"fmt\"").unwrap();
    let s = block.find("\"sort\"").unwrap();
    let t = block.find("\"time\"").unwrap();
    assert!(a < s && s < t, "sorted:\n{}", go);
    assert_eq!(block.matches("\"fmt\"").count(), 1, "deduped:\n{}", go);
}

#[test]
fn interpolation_escapes_percent_signs() {
    let go = emit("fn main() {\n    fmt.println(\"100% done, n={n}\")\n}\n");
    assert!(
        go.contains("fmt.Println(fmt.Sprintf(\"100%% done, n=%v\", n))"),
        "% escaped for Sprintf:\n{}",
        go
    );
}

#[test]
fn all_literal_interp_stays_plain_string() {
    let go = emit("fn main() {\n    fmt.println(\"no braces here\")\n}\n");
    assert!(go.contains("fmt.Println(\"no braces here\")"), ":\n{}", go);
    assert!(!go.contains("Sprintf"), "no Sprintf allocation:\n{}", go);
}

#[test]
fn cooked_strings_re_escape_and_raw_stay_verbatim() {
    let go = emit("fn main() {\n    fmt.println(\"a\\nb\")\n    fmt.println(`raw\\nstring`)\n}");
    assert!(go.contains("\"a\\nb\""), ":\n{}", go);
    assert!(go.contains("`raw\\nstring`"), "raw stays verbatim:\n{}", go);
}

#[test]
fn precedence_needs_no_parens_for_common_mixes() {
    let go = emit("fn main() {\n    x := 1 + 2 * 3\n    y := (1 + 2) * 3\n}");
    assert!(
        go.contains("x := 1 + 2 * 3"),
        "no redundant parens:\n{}",
        go
    );
    assert!(
        go.contains("y := (1 + 2) * 3"),
        "source parens kept:\n{}",
        go
    );
}

#[test]
fn bit_ops_emit_verbatim_and_parenthesize_correctly() {
    // The community benchmark report (v0.1.0) had to replace an LCG's
    // `^` with `*+` because the parser rejected SPEC grammar ops.
    let go = emit("fn main() {\n    a := b | c\n    d := e ^ f\n    g := h << 3\n    i := j >> 2\n    k := l & m\n    n := (o | p) & q\n}");
    assert!(go.contains("a := b | c"), ":\n{}", go);
    assert!(go.contains("d := e ^ f"), ":\n{}", go);
    assert!(go.contains("g := h << 3"), ":\n{}", go);
    assert!(go.contains("i := j >> 2"), ":\n{}", go);
    assert!(go.contains("k := l & m"), ":\n{}", go);
    assert!(go.contains("n := (o | p) & q"), "& tighter than |:\n{}", go);
}

#[test]
fn compound_bit_assigns_emit() {
    let go = emit("fn main() {\n    x |= y\n    x &= y\n    x ^= y\n    x <<= 1\n    x >>= 1\n}");
    assert!(go.contains("x |= y"), ":\n{}", go);
    assert!(go.contains("x &= y"), ":\n{}", go);
    assert!(go.contains("x ^= y"), ":\n{}", go);
    assert!(go.contains("x <<= 1"), ":\n{}", go);
    assert!(go.contains("x >>= 1"), ":\n{}", go);
}


#[test]
fn comparison_in_call_arg_parses_clean() {
    let go = emit("fn main() {\n    fmt.printf(\"%v\", 1 < 2)\n}");
    assert!(go.contains("fmt.Printf(\"%v\", 1 < 2)"), ":\n{}", go);
}

#[test]
fn while_and_infinite_for() {
    let go = emit("fn main() { for len(x) < 3 { } for { break } }");
    assert!(go.contains("for len(x) < 3 {"), ":\n{}", go);
    assert!(go.contains("for {"), ":\n{}", go);
    assert!(go.contains("break"), ":\n{}", go);
}

#[test]
fn if_else_if_else_chain() {
    let go = emit("fn main() {\n    if a {\n    } else if b {\n    } else {\n    }\n}\n");
    assert!(go.contains("if a {"), ":\n{}", go);
    assert!(go.contains("} else if b {"), "same-line chain:\n{}", go);
    assert!(go.contains("} else {"), ":\n{}", go);
    // Exactly one closing brace per block: no doubled braces.
    assert!(!go.contains("{\n\t\t{\n"), "no nested stray block:\n{}", go);
}

#[test]
fn if_with_init_stmt() {
    let go = emit("fn main() {\n    if v, ok := m[k]; ok {\n    }\n}\n");
    assert!(go.contains("if v, ok := m[k]; ok {"), ":\n{}", go);
}

#[test]
fn send_receive_task_and_move_drop() {
    let go = emit("fn main() {\n    ch <- move agg\n    x := <-ch\n    task worker(x)\n}");
    assert!(go.contains("ch <- agg"), "move marker dropped:\n{}", go);
    assert!(go.contains("x := <-ch"), ":\n{}", go);
    assert!(go.contains("go worker(x)"), ":\n{}", go);
}

#[test]
fn var_and_const_locals() {
    let go = emit("fn main() {\n    var got []int\n    var hits atomic.int64\n    const n = 4\n}");
    assert!(go.contains("var got []int"), ":\n{}", go);
    assert!(
        go.contains("var hits atomic.Int64"),
        "dotted foreign type normalized:\n{}",
        go
    );
    assert!(go.contains("const n = 4"), ":\n{}", go);
}

#[test]
fn named_results_on_plain_fn() {
    let go = emit("fn f() (code int) {\n    return 3\n}\n");
    assert!(go.contains("func f() (code int) {"), ":\n{}", go);
    assert!(go.contains("return 3"), ":\n{}", go);
}

#[test]
fn method_receiver_and_struct_methods() {
    let go = emit("struct g {\n    n int\n}\n\nfn (p *g) add(x int) {\n    p.n += x\n}\n");
    assert!(go.contains("func (p *g) add(x int) {"), ":\n{}", go);
}

#[test]
fn addr_of_and_index_inc() {
    // (`*p` deref is not in the parser's MVP surface — pointer structs use
    // auto-deref field access, exactly like Go.)
    let go = emit("fn main() {\n    p := &x\n    m[k]++\n}");
    assert!(go.contains("p := &x"), ":\n{}", go);
    assert!(go.contains("m[k]++"), ":\n{}", go);
}

#[test]
fn multi_define_from_two_value_call() {
    let go = emit("fn main() {\n    a, b := f()\n}");
    assert!(go.contains("a, b := f()"), ":\n{}", go);
}

#[test]
fn struct_literal_fields_keep_our_names() {
    let go = emit("struct job {\n    cost_ms int\n}\n\nfn main() {\n    j := job{cost_ms: 3}\n}\n");
    assert!(
        go.contains("job{cost_ms: 3}"),
        "our fields NOT camelized:\n{}",
        go
    );
}

#[test]
fn slice_expr_forms() {
    let go = emit("fn main() {\n    a := xs[1:]\n    b := xs[:2]\n    c := xs[1:2]\n}");
    assert!(go.contains("a := xs[1:]"), ":\n{}", go);
    assert!(go.contains("b := xs[:2]"), ":\n{}", go);
    assert!(go.contains("c := xs[1:2]"), ":\n{}", go);
}

#[test]
fn spread_argument() {
    let go = emit("fn main() {\n    f(xs...)\n}");
    assert!(go.contains("f(xs...)"), ":\n{}", go);
}

#[test]
fn managed_block_emits_marker_comment() {
    let go = emit(
        "fn main() {\n    managed(\"shared counter under a lock\", \"mutex\", \"PROJ-9\") {\n        task w(counter)\n    }\n}\n",
    );
    assert!(go.contains("// managed"), "audit marker greppable:\n{}", go);
    // row 14: the marker carries the category + reason + tickets
    assert!(go.contains("[mutex]"), "category in marker:\n{}", go);
    assert!(go.contains("shared counter under a lock"), "reason in marker:\n{}", go);
    assert!(go.contains("PROJ-9"), "tickets in marker:\n{}", go);
    assert!(go.contains("go w(counter)"), ":\n{}", go);
}

#[test]
fn main_without_results_or_try_stays_plain() {
    let go = emit("fn main() {\n    fmt.println(\"x\")\n}\n");
    assert!(go.contains("func main() {"), ":\n{}", go);
    assert!(!go.contains("func() ("), "no wrapper closure:\n{}", go);
    assert!(!go.contains("os.Exit"), "no exit plumbing:\n{}", go);
}

#[test]
fn main_with_try_but_no_results_gets_wrapper() {
    let go = emit("fn main() {\n    f := os.open(p)?\n}\n");
    assert!(go.contains("func main() {"), ":\n{}", go);
    assert!(
        go.contains("func() (hyErr error) {"),
        "error-only closure:\n{}",
        go
    );
    assert!(go.contains("os.Exit(1)"), ":\n{}", go);
    assert!(!go.contains("os.Exit(code)"), "no code var:\n{}", go);
}

// ---------------------------------------------------------------------------
// Clean unsupported errors — never a wrong emission
// ---------------------------------------------------------------------------

#[test]
fn rust_crate_import_errors_cleanly() {
    let msg = emit_err("import \"rust/notacrate\" fn main() { }");
    // serde_json + sha3 are pinned (Batches F/G); an UNKNOWN crate must
    // name the binding set, never silently emit.
    assert!(
        msg.contains("not in the pinned binding set"),
        "points at the binding discipline: {}",
        msg
    );
}

#[test]
fn enum_decl_errors_cleanly() {
    let msg = emit_err("enum shape { circle square } fn main() { }");
    assert!(msg.contains("enum"), "{}", msg);
}

#[test]
fn match_stmt_errors_cleanly() {
    let msg = emit_err("fn main() { match x { _ => { } } }");
    assert!(msg.contains("match"), "{}", msg);
}

#[test]
fn select_emits_go_cases() {
    let go = emit(
        "fn main() {\n    select {\n        v, ok := <-sq:\n            _ = ok\n        <-deadline:\n            _ = deadline\n    }\n}\n",
    );
    assert!(go.contains("select {"), ":\n{}", go);
    assert!(go.contains("case v, ok := <-sq:"), ":\n{}", go);
    assert!(go.contains("case <-deadline:"), ":\n{}", go);
}

#[test]
fn inclusive_range_and_labeled_break() {
    // Example 04's exact shapes: `1..=8` counts 8; `break loop` inside a
    // select breaks the LOOP (Go label), not the select.
    let go = emit(
        "fn main() {\n    total := 0\n    loop {\n        select {\n            v, ok := <-sq:\n                if !ok { break loop }\n                total += v\n        }\n    }\n    for i in 1..=8 {\n        total += i\n    }\n}\n",
    );
    assert!(
        go.contains("hyLoop:\n\tfor {"),
        "loop gets the Go label on its own line (gofmt canonical):\n{}",
        go
    );
    assert!(go.contains("break hyLoop"), ":\n{}", go);
    assert!(
        go.contains("for i := 1; i <= 8; i++ {"),
        "inclusive range:\n{}",
        go
    );
}

#[test]
fn sibling_labeled_loops_get_distinct_go_labels() {
    // Two SIBLING loops that each own a `break loop` are both legal source
    // (LX14 binds each label to its own loop), but a fixed `hyLoop` name
    // emitted two declarations → `go vet`: "label hyLoop already declared",
    // so an accepted program could never build.
    let go = emit(
        "fn main() {\n    for i in 0..2 {\n        break loop\n    }\n    for j in 0..2 {\n        break loop\n    }\n}\n",
    );
    assert_eq!(
        go.matches("hyLoop:").count(),
        1,
        "outer label keeps the canonical name:\n{}",
        go
    );
    assert_eq!(
        go.matches("hyLoop1:").count(),
        1,
        "the sibling gets a fresh label:\n{}",
        go
    );
    assert!(!go.contains("hyLoop2"), "no wasted label counter:\n{}", go);
    assert!(go.contains("break hyLoop\n"), ":\n{}", go);
    assert!(go.contains("break hyLoop1\n"), ":\n{}", go);
}

#[test]
fn nested_labeled_loops_break_their_own_loop() {
    // A labeled `break loop` inside a NESTED loop targets the nested loop
    // (LX14), so each level carries its own label and each break must name
    // the one it is lexically inside.
    let go = emit(
        "fn main() {\n    for i in 0..2 {\n        for j in 0..2 {\n            break loop\n        }\n        break loop\n    }\n}\n",
    );
    assert_eq!(go.matches("hyLoop:").count(), 1, ":\n{}", go);
    assert_eq!(go.matches("hyLoop1:").count(), 1, ":\n{}", go);
    assert_eq!(
        go.matches("break hyLoop1").count(),
        1,
        "the nested break names the nested loop:\n{}",
        go
    );
    assert_eq!(
        go.matches("break hyLoop\n").count(),
        1,
        "the outer break names the outer loop:\n{}",
        go
    );
}

#[test]
fn exclusive_range_stays_exclusive() {
    let go = emit("fn main() {\n    for i in 0..n {\n    }\n}\n");
    assert!(go.contains("for i := 0; i < n; i++ {"), ":\n{}", go);
    assert!(
        !go.contains("hyLoop"),
        "no label without a labeled break:\n{}",
        go
    );
}

#[test]
fn panic_statement_form() {
    let go = emit("fn main() {\n    panic \"pipeline timed out\"\n}\n");
    assert!(go.contains("panic(\"pipeline timed out\")"), ":\n{}", go);
}

#[test]
fn type_elided_struct_lit_errors_cleanly() {
    let msg = emit_err("fn main() { append(rows, {name: 1}) }");
    assert!(msg.contains("type-elided"), "{}", msg);
}

#[test]
fn try_outside_binding_position_errors_cleanly() {
    // The E3 expression form hoists inside main / handler closures; outside
    // both, propagation has nowhere to land — a clean error, never a wrong
    // emission (the hoister's propagate_error names the surface).
    let msg = emit_err("fn f() { fmt.println(g()?) }");
    assert!(msg.contains("`?` outside main"), "{}", msg);
}

// ---------------------------------------------------------------------------
// Batch D — option/match, generics, type aliases, slot loans, set/pop
// ---------------------------------------------------------------------------

#[test]
fn type_alias_emits_go_alias() {
    let go = emit("type node_id = int\n\nfn main() {\n    x := node_id(3)\n}\n");
    assert!(go.contains("type node_id = int"), ":\n{}", go);
}

#[test]
fn generic_fn_and_struct_type_params() {
    let go = emit(
        "struct box[t any] {\n    v t\n}\n\nfn box.new[t any](v t) box[t] {\n    return box[t]{v: v}\n}\n",
    );
    assert!(go.contains("type box[t any] struct {"), ":\n{}", go);
    assert!(
        go.contains("func box_new[t any](v t) box[t] {"),
        "generic assoc fn with mangled name:\n{}",
        go
    );
}

#[test]
fn assoc_fn_call_mangles_dotted_name() {
    let go = emit(
        "struct box[t any] {\n    v t\n}\n\nfn box.new[t any](v t) box[t] {\n    return box[t]{v: v}\n}\n\nfn main() {\n    b := box.new[int](3)\n}\n",
    );
    assert!(go.contains("box_new[int](3)"), ":\n{}", go);
}

#[test]
fn option_match_lowes_to_ok_branch() {
    let go = emit(
        "fn get() option[int] {\n    return some(3)\n}\n\nfn main() {\n    match get() {\n        some(v): fmt.println(\"v={v}\")\n        none:     fmt.println(\"none\")\n    }\n}\n",
    );
    assert!(
        go.contains("type option[v any] struct {"),
        "option prelude:\n{}",
        go
    );
    assert!(
        go.contains("func some[v any](val v) option[v] {"),
        ":\n{}",
        go
    );
    assert!(
        go.contains("func get() option[int] {"),
        "option result bare:\n{}",
        go
    );
    assert!(go.contains("return some(3)"), ":\n{}", go);
    let m = go.find("hyMatch0 := get()").expect("match temp");
    let body = &go[m..];
    assert!(body.contains("if hyMatch0.ok {"), ":\n{}", go);
    assert!(body.contains("v := hyMatch0.val"), ":\n{}", go);
    assert!(body.contains("} else {"), ":\n{}", go);
    assert!(
        body.contains("fmt.Println(fmt.Sprintf(\"v=%v\", v))"),
        ":\n{}",
        go
    );
}

#[test]
fn match_multi_some_arms_retains_all_arms() {
    // Issue #204: match on option with multiple some arms must retain all arms,
    // not just the last one.
    let go = emit(
        "fn main() {\n    opt := some(15)\n    match opt {\n        some(v) if v > 10: {\n            fmt.println(\"big\")\n        }\n        some(v): {\n            fmt.println(\"small\")\n        }\n        none: {\n            fmt.println(\"none\")\n        }\n    }\n}\n",
    );
    assert!(go.contains("if hyMatch0.ok {"), ":\n{}", go);
    assert!(go.contains("v := hyMatch0.val"), ":\n{}", go);
    assert!(go.contains("if v > 10 {"), ":\n{}", go);
    assert!(go.contains("fmt.Println(\"big\")"), ":\n{}", go);
    assert!(go.contains("fmt.Println(\"small\")"), ":\n{}", go);
    assert!(go.contains("fmt.Println(\"none\")"), ":\n{}", go);
}

#[test]
fn match_guarded_some_with_wildcard_fallback() {
    // Issue #204: with guarded some arms and wildcard, failed guard must fall through to wildcard.
    let go = emit(
        "fn main() {\n    opt := some(3)\n    match opt {\n        some(v) if v > 10: {\n            fmt.println(\"big\")\n        }\n        _: {\n            fmt.println(\"fallback\")\n        }\n    }\n}\n",
    );
    assert!(go.contains("hyMatched1 := false"), ":\n{}", go);
    assert!(go.contains("if hyMatch0.ok {"), ":\n{}", go);
    assert!(go.contains("if v > 10 {"), ":\n{}", go);
    assert!(go.contains("hyMatched1 = true"), ":\n{}", go);
    assert!(go.contains("fmt.Println(\"big\")"), ":\n{}", go);
    assert!(go.contains("if !hyMatched1 {"), ":\n{}", go);
    assert!(go.contains("fmt.Println(\"fallback\")"), ":\n{}", go);
}

#[test]
fn match_guarded_some_with_none_and_wildcard() {
    // Issue #204: guarded some arm, none arm, and wildcard arm all handled cleanly.
    let go = emit(
        "fn main() {\n    opt := some(3)\n    match opt {\n        some(v) if v > 10: {\n            fmt.println(\"big\")\n        }\n        none: {\n            fmt.println(\"none\")\n        }\n        _: {\n            fmt.println(\"fallback\")\n        }\n    }\n}\n",
    );
    assert!(go.contains("hyMatched1 := false"), ":\n{}", go);
    assert!(go.contains("if hyMatch0.ok {"), ":\n{}", go);
    assert!(go.contains("if v > 10 {"), ":\n{}", go);
    assert!(go.contains("hyMatched1 = true"), ":\n{}", go);
    assert!(go.contains("fmt.Println(\"big\")"), ":\n{}", go);
    assert!(go.contains("if !hyMatched1 && !hyMatch0.ok {"), ":\n{}", go);
    assert!(go.contains("fmt.Println(\"none\")"), ":\n{}", go);
    assert!(go.contains("if !hyMatched1 {"), ":\n{}", go);
    assert!(go.contains("fmt.Println(\"fallback\")"), ":\n{}", go);
}

#[test]
fn return_none_lowers_to_option_zero() {
    let go = emit("fn get() option[int] {\n    return none\n}\n");
    assert!(
        go.contains("return option[int]{}"),
        "none → zero option:\n{}",
        go
    );
}

#[test]
fn slot_loan_copies_and_stores_back() {
    // O8's map-slot loan: Go cannot &m[k]; copy-read now + store-back at
    // the declaring block's end.
    let go = emit(
        "struct node {\n    edges []int\n}\n\nstruct graph {\n    nodes map[int]node\n}\n\nfn (g *graph) connect(from int, to int) {\n    n := &g.nodes[from]\n    n.edges = append(n.edges, to)\n}\n",
    );
    assert!(
        go.contains("n := g.nodes[from]"),
        "copy read (no &):\n{}",
        go
    );
    assert!(go.contains("n.edges = append(n.edges, to)"), ":\n{}", go);
    assert!(
        go.contains("g.nodes[from] = n"),
        "store-back at block end:\n{}",
        go
    );
}

#[test]
fn assign_slot_loan_copies_and_stores_back() {
    // Issue #138: re-assignment `n = &g.nodes[from]` lowers to copy-read
    // and schedules store-back at block's end.
    let go = emit(
        "struct node {\n    edges []int\n}\n\nstruct graph {\n    nodes map[int]node\n}\n\nfn (g *graph) connect(from int, to int) {\n    var n node\n    n = &g.nodes[from]\n    n.edges = append(n.edges, to)\n}\n",
    );
    assert!(
        go.contains("n = g.nodes[from]"),
        "copy assign (no &):\n{}",
        go
    );
    assert!(go.contains("n.edges = append(n.edges, to)"), ":\n{}", go);
    assert!(
        go.contains("g.nodes[from] = n"),
        "store-back at block end:\n{}",
        go
    );
}

#[test]
fn single_var_iterate_value_semantics() {
    // Slices: `for w in shard` binds the VALUE (Go's range binds the index).
    let go = emit("fn f(shard []int) {\n    for w in shard {\n        _ = w\n    }\n}\n");
    assert!(go.contains("for _, w := range shard {"), ":\n{}", go);
}

#[test]
fn single_var_iterate_over_chan_binds_value() {
    let go = emit("fn worker(jobs <-chan int) {\n    for j in jobs {\n        _ = j\n    }\n}\n");
    assert!(
        go.contains("for j := range jobs {"),
        "chan keeps direct range:\n{}",
        go
    );
}

#[test]
fn set_insert_uses_prelude() {
    let go = emit(
        "type node_id = int\n\nfn f() bool {\n    seen := set[node_id]{}\n    return seen.insert(3)\n}\n",
    );
    assert!(
        go.contains("type set[t comparable] struct {"),
        "set prelude:\n{}",
        go
    );
    assert!(go.contains("seen := set[node_id]{}"), ":\n{}", go);
    assert!(go.contains("return seen.insert(3)"), ":\n{}", go);
}

#[test]
fn slice_pop_uses_prelude_helper() {
    let go =
        emit("type node_id = int\n\nfn f(stack []node_id) node_id {\n    return stack.pop()\n}\n");
    assert!(
        go.contains("func hy_pop[t any](s *[]t) t {"),
        "pop prelude:\n{}",
        go
    );
    assert!(go.contains("return hy_pop(&stack)"), ":\n{}", go);
}

// ---------------------------------------------------------------------------
// Batch E — the foreign shape table (example 06: net/http surface)
// ---------------------------------------------------------------------------

#[test]
fn shape_chain_field_method_field() {
    // r.url.query.get("a") — FIELD (r.URL), zero-arg METHOD (Query()),
    // METHOD (Get): the exact three-level chain from example 06.
    let go = emit("fn f(r *http.request) string {\n    return r.url.query.get(\"a\")\n}\n");
    assert!(
        go.contains("r.URL.Query().Get(\"a\")"),
        "typed chain lowered exactly:\n{}",
        go
    );
    assert!(!go.contains("r.Url"), "naive M4 CamelCase leaked:\n{}", go);
}

#[test]
fn shape_fields_stay_fields() {
    let go = emit("fn f(r *http.request) {\n    fmt.eprintln(\"{r.method} {r.url.path}\")\n}\n");
    assert!(go.contains("r.Method"), ":\n{}", go);
    assert!(go.contains("r.URL.Path"), ":\n{}", go);
    assert!(!go.contains("r.Url"), ":\n{}", go);
}

#[test]
fn shape_import_paths_map_to_net_packages() {
    let go = emit(
        "fn f(r *http.request) {\n    mux := http.new_serve_mux()\n    mux.handle(\"/x\", http.handler(fn(w, r) { }))\n}\n",
    );
    assert!(go.contains("\"net/http\""), "mapped import path:\n{}", go);
    assert!(
        !go.contains("\n\t\"http\"\n"),
        "unmapped root import leaked:\n{}",
        go
    );
}

#[test]
fn shape_handler_conversion_types_untyped_params() {
    // http.handler(fn(w, r) { … }) — the literal's untyped params take the
    // TARGET's signature: w ResponseWriter (interface), r *Request (pointer).
    let go = emit(
        "fn f(mux *http.serve_mux) {\n    mux.handle(\"/x\", http.handler(fn(w, r) {\n        fmt.fprintln(w, \"hi\")\n    }))\n}\n",
    );
    assert!(
        go.contains("http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {"),
        "conversion + typed params:\n{}",
        go
    );
    assert!(
        !go.contains("*http.ResponseWriter"),
        "interface must not be a pointer:\n{}",
        go
    );
}

#[test]
fn shape_handler_conversion_count_mismatch_is_clean_error() {
    let msg = emit_err(
        "fn f() {\n    mux := http.new_serve_mux()\n    mux.handle(\"/x\", http.handler(fn(w) { }))\n}\n",
    );
    assert!(
        msg.contains("target's signature has"),
        "clean arity error:\n{}",
        msg
    );
}

#[test]
fn shape_e11_handler_try_propagates_through_http_error() {
    // E11: `?` inside an http.handler closure has no error result to land
    // in — the target's propagation contract is http.Error + return.
    let go = emit(
        "fn f() {\n    mux := http.new_serve_mux()\n    mux.handle(\"/x\", http.handler(fn(w, r) {\n        n := strconv.atoi(\"3\")?\n        fmt.fprintln(w, n)\n    }))\n}\n",
    );
    assert!(
        go.contains("http.Error(w, hyErr0.Error(), http.StatusBadRequest)"),
        "E11 propagation:\n{}",
        go
    );
    assert!(
        !go.contains("hyErr = hyErr0"),
        "E3 shape must not leak:\n{}",
        go
    );
}

#[test]
fn shape_httptest_get_composite_lowering() {
    let go = emit(
        "fn main() {\n    mux := http.new_serve_mux()\n    code, body := httptest.get(mux, \"/hello\")?\n    fmt.println(\"{code} {body}\")\n}\n",
    );
    assert!(
        go.contains("http.NewRequest(\"GET\", \"/hello\", nil)"),
        ":\n{}",
        go
    );
    assert!(go.contains("httptest.NewRecorder()"), ":\n{}", go);
    assert!(go.contains("mux.ServeHTTP("), ":\n{}", go);
    assert!(
        go.contains("strings.TrimSuffix("),
        "body-line trim pinned by example 06's expected output:\n{}",
        go
    );
}

#[test]
fn shape_httptest_get_bare_call_is_clean_error() {
    let msg = emit_err(
        "fn main() {\n    mux := http.new_serve_mux()\n    x := httptest.get(mux, \"/h\")\n}\n",
    );
    assert!(msg.contains("bind it with `?`"), ":\n{}", msg);
}

#[test]
fn shape_return_fnlit_wraps_in_handler_func() {
    // `return fn(...)` where the fn's result is http.handler — Go's
    // interface needs the HandlerFunc conversion at the return.
    let go = emit(
        "fn logging(next http.handler) http.handler {\n    return fn(w http.response_writer, r *http.request) {\n        next.serve_http(w, r)\n    }\n}\n",
    );
    assert!(
        go.contains("return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {"),
        ":\n{}",
        go
    );
    assert!(
        go.contains("next.ServeHTTP(w, r)"),
        "interface method via shape table:\n{}",
        go
    );
}

#[test]
fn shape_e6_wrapper_terminal_return_after_range_for() {
    // A main whose wrapper body ends in a range-for (non-terminating in
    // Go) — the wrapper's named result carries a terminal bare `return`.
    // (Without `?`/results there is no wrapper and no requirement.)
    let go = emit(
        "fn main() {\n    for _, p in [\"/a\"] {\n        n := strconv.atoi(p)?\n        fmt.println(n)\n    }\n}\n",
    );
    assert!(
        go.contains("\n\t\treturn\n\t}()"),
        "terminal return before the wrapper close:\n{}",
        go
    );
}

// ---------------------------------------------------------------------------
// Batch F — the rust-crate bridge surface (example 10)
// ---------------------------------------------------------------------------

#[test]
fn rust_call_lowers_to_binding_with_bytes_coerce() {
    let go = emit(
        "import \"rust/sha3\"\n\nfn main() {\n    digest := sha3.digest256(\"hybrid\")?\n    fmt.println(digest)\n}\n",
    );
    assert!(
        go.contains("sha3binding.Digest256([]byte(\"hybrid\"))"),
        "bytes coercion at the binding call:\n{}",
        go
    );
    assert!(go.contains("\"hybrid.program/sha3binding\""), ":\n{}", go);
    assert!(
        !go.contains("\"rust/sha3\""),
        "rust import must not leak:\n{}",
        go
    );
}

#[test]
fn rust_named_arg_places_by_descriptor_order() {
    // `stream_checksum(1_000_000, seed=7)` — Go has no named args; the
    // descriptor's param order (iters, seed) places them positionally.
    let go = emit(
        "import \"rust/sha3\"\n\nfn main() {\n    c := sha3.stream_checksum(1000000, seed=7)?\n    fmt.println(c)\n}\n",
    );
    assert!(
        go.contains("sha3binding.StreamChecksum(1000000, 7)"),
        ":\n{}",
        go
    );
}

#[test]
fn rust_unknown_fn_is_clean_error() {
    let msg = emit_err("import \"rust/sha3\"\n\nfn main() {\n    x := sha3.nope(1)?\n}\n");
    assert!(
        msg.contains("not in the pinned binding surface"),
        ":\n{}",
        msg
    );
}

#[test]
fn rust_unknown_arg_name_is_clean_error() {
    let msg = emit_err(
        "import \"rust/sha3\"\n\nfn main() {\n    c := sha3.stream_checksum(1, seedx=7)?\n}\n",
    );
    assert!(msg.contains("no parameter named `seedx`"), ":\n{}", msg);
}

#[test]
fn hex_encode_maps_to_encode_to_string() {
    let go = emit("fn f(d []byte) {\n    fmt.println(hex.encode(d[..8]))\n}\n");
    assert!(go.contains("hex.EncodeToString(d[:8])"), ":\n{}", go);
    assert!(go.contains("\"encoding/hex\""), ":\n{}", go);
}

#[test]
fn duration_members_resolve_property_style() {
    // dt := time.since(t0) — dt is a KNOWN duration: `.milliseconds` is a
    // zero-arg METHOD in Go (MethodZero), not a field.
    let go = emit(
        "fn f() {\n    t0 := time.now()\n    dt := time.since(t0)\n    fmt.println(\"{dt.milliseconds}ms\")\n}\n",
    );
    assert!(go.contains("dt.Milliseconds()"), ":\n{}", go);
    assert!(
        !go.contains("dt.Milliseconds\n"),
        "field spelling leaked:\n{}",
        go
    );
}

#[test]
fn rust_crate_import_decl_registers_nothing_unknown_errors() {
    let msg = emit_err("import \"rust/notpinned\"\n\nfn f() { }\n");
    assert!(msg.contains("not in the pinned binding set"), ":\n{}", msg);
}

#[test]
fn rust_json_handle_types_map_to_binding_package() {
    // serde_json.value → jsonbinding.Value (a fn's result type).
    let go = emit(
        "import \"rust/serde_json\"\n\nfn users_json(names []string) serde_json.value {\n    v := serde_json.new_object()\n    let arr := serde_json.new_array()\n    v.set(\"active\", arr)\n    return v\n}\n",
    );
    assert!(
        go.contains("func users_json(names []string) jsonbinding.Value {"),
        "handle type spelling:\n{}",
        go
    );
    assert!(go.contains("v := jsonbinding.NewObject()"), ":\n{}", go);
    assert!(go.contains("arr := jsonbinding.NewArray()"), ":\n{}", go);
    assert!(go.contains("v.Set(\"active\", arr)"), ":\n{}", go);
    assert!(
        !go.contains("serde_json."),
        "rust crate spelling leaked:\n{}",
        go
    );
}

#[test]
fn e3_expression_form_hoists_in_conditions_and_args() {
    // `if u.int("logins")? >= 3` + `append(names, u.string("name")?)` —
    // the E3 desugar applied expression-wise (example 05's exact shapes).
    let go = emit(
        "import \"rust/serde_json\"\n\nfn main() {\n    doc := serde_json.parse(\"[]\")?\n    var names []string\n    for u in doc.array(\"users\")? {\n        if u.int(\"logins\")? >= 3 {\n            names = append(names, u.string(\"name\")?)\n        }\n    }\n}\n",
    );
    assert!(go.contains("u.Int(\"logins\")"), ":\n{}", go);
    assert!(go.contains("u.String(\"name\")"), ":\n{}", go);
    assert!(go.contains("names = append(names,"), ":\n{}", go);
    assert!(
        go.contains("if hyT3 >= 3"),
        "cond hoisted to a temp:\n{}",
        go
    );
}

#[test]
fn e3_for_iterate_over_array_try_hoists_before_loop() {
    let go = emit(
        "import \"rust/serde_json\"\n\nfn main() {\n    doc := serde_json.parse(\"[]\")?\n    for u in doc.array(\"users\")? {\n        fmt.println(u)\n    }\n}\n",
    );
    // The hoist precedes the for; the loop ranges the temp; the loop var
    // binds the handle type (u.Int would resolve — no verbatim leak).
    let hoist = go.find("hyT1, hyErr").unwrap();
    let loop_at = go.find("for _, u := range hyT1").unwrap();
    assert!(hoist < loop_at, "hoist must precede the loop:\n{}", go);
}

// ---------------------------------------------------------------------------
// Batch H — package emission (example 09's adoption story)
// ---------------------------------------------------------------------------

#[test]
fn package_mode_exports_pub_fns() {
    let file = hy_syntax::parse_file(
        "t.hy",
        "import \"rust/regex\"\n\npub fn validate_email(s string) (string, error) {\n    ok := regex.is_match(`^x$`, s)?\n    if !ok {\n        return _, errors.new(\"invalid email address\")\n    }\n    return s, nil\n}\n\npub fn simple_pattern(p string) bool {\n    return regex.is_valid(p)\n}\n",
    )
    .expect("parse");
    let go = super::emit_go_package(&file, "hybrid", "example.com/app/hybrid").expect("emit");
    assert!(
        go.contains("func ValidateEmail(s string) (string, error) {"),
        "M4 export mapping:\n{}",
        go
    );
    assert!(
        go.contains("func SimplePattern(p string) bool {"),
        ":\n{}",
        go
    );
    assert!(go.contains("package hybrid\n"), ":\n{}", go);
    // E3 in an error-result fn propagates via the zero-results return.
    assert!(
        go.contains("return \"\", hyErr0"),
        "zero-fill propagation:\n{}",
        go
    );
    // `return _, err` fills the `_` slot with the zero value.
    assert!(go.contains("return \"\", errors.New("), ":\n{}", go);
    // Bindings import under the consumer's prefix.
    assert!(
        go.contains("\"example.com/app/hybrid/regexbinding\""),
        ":\n{}",
        go
    );
    assert!(go.contains("regexbinding.IsMatch("), ":\n{}", go);
    assert!(
        !go.contains("[]byte("),
        "regex binding takes strings:\n{}",
        go
    );
}

#[test]
fn package_mode_private_fns_stay_snake_case() {
    let file =
        hy_syntax::parse_file("t.hy", "fn helper(n int) int {\n    return n\n}\n").expect("parse");
    let go = super::emit_go_package(&file, "hybrid", "x").expect("emit");
    assert!(go.contains("func helper(n int) int {"), ":\n{}", go);
}

// ---------------------------------------------------------------------------
// Batch I — the differential corpus loop's fixes (each pinned by an
// accepted checker-corpus program that previously could not execute)
// ---------------------------------------------------------------------------

#[test]
fn diff_blank_range_for_uses_fresh_temp() {
    // corpus 16/17/18: `for _ in 0..n` — Go has no blank := variable; the
    // old emission `for _ := 0; _ < n; _++` was invalid Go.
    let go =
        emit("fn main() {\n    n := 3\n    for _ in 0..n {\n        fmt.println(\"x\")\n    }\n}");
    assert!(
        go.contains("for hyBlank0 := 0; hyBlank0 < n; hyBlank0++ {"),
        "fresh temp carries the count:\n{}",
        go
    );
    assert!(!go.contains("for _ :="), "blank := leaked:\n{}", go);
}

#[test]
fn diff_blank_iterate_emits_for_range() {
    // corpus 17: `for _ in 0..n_probes { s := <-samples }` — the drain form.
    let go = emit("fn main() {\n    done := make(chan bool, 2)\n    for _ in done {\n    }\n}");
    assert!(go.contains("for range done {"), ":\n{}", go);
}

#[test]
fn diff_mutex_methods_normalize_via_struct_field() {
    // corpus 18: `stats.mu.lock()` — receiver typing through var_struct +
    // struct_fields; the shape table carries sync.mutex's exact Go names.
    let go = emit(
        "struct pool_stats {\n    mu sync.mutex\n}\n\nfn main() {\n    stats := pool_stats{}\n    stats.mu.lock()\n    stats.mu.unlock()\n}",
    );
    assert!(go.contains("stats.mu.Lock()"), ":\n{}", go);
    assert!(go.contains("stats.mu.Unlock()"), ":\n{}", go);
    assert!(!go.contains(".lock()"), "lowercase leaked:\n{}", go);
}

#[test]
fn diff_atomic_int64_methods() {
    // corpus 17: `var hits atomic.int64` + `hits.add(1)` / `hits.load()` —
    // Go's atomic.Int64 struct methods, auto-addressed on the value.
    let go = emit(
        "fn probe(hits *atomic.int64) {\n    hits.add(1)\n}\n\nfn main() {\n    var hits atomic.int64\n    probe(&hits)\n    fmt.println(\"{hits.load()}\")\n}",
    );
    assert!(go.contains("var hits atomic.Int64"), ":\n{}", go);
    assert!(go.contains("hits.Add(1)"), ":\n{}", go);
    assert!(go.contains("hits.Load()"), ":\n{}", go);
    assert!(go.contains("\"sync/atomic\""), "import path:\n{}", go);
}

#[test]
fn diff_zero_result_try_hoist_has_no_value_temp() {
    // corpus 12: `doc.set("audited", true)?` — Set returns only error; the
    // hoist emits `hyErrN := call` (no temp, no bare-temp statement).
    let go = emit(
        "fn main() {\n    doc := serde_json.parse(`{\"a\":1}`)?\n    doc.set(\"audited\", true)?\n}",
    );
    assert!(
        go.contains("hyErr1 := doc.Set(\"audited\", true)"),
        ":\n{}",
        go
    );
    assert!(!go.contains("hyT"), "value temp leaked:\n{}", go);
}

#[test]
fn diff_elided_map_value_struct_literal() {
    // corpus 09: `map[string]service{"k": {…}}` — the elided value inherits
    // the map's VALUE type (Go's own composite-literal sugar).
    let go = emit(
        "struct service {\n    replicas int\n    tag string\n}\n\nfn main() {\n    e := env{services: map[string]service{\n        \"frontend\": {replicas: 2, tag: \"prod\"},\n    }}\n    _ = e\n}\n\nstruct env {\n    services map[string]service\n}",
    );
    assert!(
        go.contains("\"frontend\": service{replicas: 2, tag: \"prod\"}"),
        "elided head filled:\n{}",
        go
    );
}

#[test]
fn diff_short_return_fills_error_slot() {
    // corpus 13: `return v` in a (value, error) fn → `return v, nil`.
    let go = emit(
        "fn summary() (serde_json.value, error) {\n    v := serde_json.new_object()\n    return v\n}\n\nfn main() {\n    s := summary()?\n    _ = s\n}",
    );
    assert!(go.contains("return v, nil"), ":\n{}", go);
}

#[test]
fn diff_composite_const_lands_as_var() {
    // corpus 17/18: `const thresholds = [10, 50, 250, 1000]` — Go's const
    // admits only bool/string/number; composites lower to var.
    let go = emit("const thresholds = [10, 50, 250, 1000]\n\nfn main() {\n    fmt.println(\"{thresholds[0]}\")\n}");
    assert!(
        go.contains("var thresholds = []int{10, 50, 250, 1000}"),
        ":\n{}",
        go
    );
    assert!(!go.contains("const thresholds"), ":\n{}", go);
}

#[test]
fn local_composite_const_lands_as_var() {
    let go = emit("fn main() {\n    const thresholds = [10, 50]\n    fmt.println(\"{thresholds[0]}\")\n}");
    assert!(
        go.contains("var thresholds = []int{10, 50}"),
        ":\n{}",
        go
    );
    assert!(!go.contains("const thresholds"), ":\n{}", go);
}

#[test]
fn diff_pointer_param_auto_address() {
    // corpus 09: `scale_then_register(e, …)` with `e *env` — an addressable
    // argument auto-addresses at a pointer param (Go's method-receiver
    // precedent, pinned by the pre-registered corpus).
    let go = emit(
        "fn scale(e *env) {\n    _ = e\n}\n\nfn main() {\n    e := env{}\n    scale(e)\n}\n\nstruct env {\n    n int\n}",
    );
    assert!(go.contains("scale(&e)"), ":\n{}", go);
}

#[test]
fn diff_hyfn_rust_typed_result_normalizes_methods() {
    // corpus 13: `s := summary(doc)?` — summary's declared result is
    // serde_json.value; `s.set(...)` normalizes like any rust-call result.
    let go = emit(
        "fn summary() (serde_json.value, error) {\n    v := serde_json.new_object()\n    return v\n}\n\nfn main() {\n    s := summary()?\n    s.set(\"cached_at\", \"2026-10-12\")?\n}",
    );
    assert!(
        go.contains("s.Set(\"cached_at\", \"2026-10-12\")"),
        ":\n{}",
        go
    );
    assert!(!go.contains("s.set("), "lowercase leaked:\n{}", go);
}

#[test]
fn diff_duplicate_hyerr_assign_gone() {
    // The main-closure propagation pushed `hyErr = t` twice (manual push +
    // propagate_error's branch); one is the contract.
    let go = emit(
        "fn main() {\n    n := count()?\n    fmt.println(\"{n}\")\n}\n\nfn count() (int, error) {\n    return 1, nil\n}",
    );
    let count = go.matches("hyErr = hyErr0").count();
    assert_eq!(count, 1, "exactly one propagation assign:\n{}", go);
}

// ---------------------------------------------------------------------------

// The unbound-crate refusal (the adoption-audit flate2 follow-up)
// ---------------------------------------------------------------------------

#[test]
fn unbound_crate_call_refused_honestly() {
    // The checker's foreign table knows flate2 (the A7 fixture), so `hyb
    // check` passes — but the runtime ships no binding. A call WITHOUT an
    // explicit import used to fall through emit_call's package-root gate,
    // emit verbatim, and die at go build with `undefined: flate2`.
    let msg = emit_err("fn main() {\n    enc := flate2.new_writer(6)?\n    fmt.println(enc)\n}");
    assert!(
        msg.contains("does not ship a `flate2` binding"),
        "honest message expected:\n{}",
        msg
    );
    assert!(
        !msg.contains("not yet supported"),
        "must not read as a language gap:\n{}",
        msg
    );
}

#[test]
fn unbound_crate_import_refused_honestly() {
    // The explicit-import shape used to say "not in the pinned binding
    // set" — true but misleading (it reads like the crate is merely
    // unpinned, when in fact the checker knows it and the runtime
    // deliberately does not ship it).
    let msg = emit_err("import \"rust/flate2\"\n\nfn main() {\n    enc := flate2.new_writer(6)?\n    fmt.println(enc)\n}");
    assert!(
        msg.contains("does not ship a `flate2` binding"),
        "honest message expected:\n{}",
        msg
    );
}

// ---------------------------------------------------------------------------
// issue #139 - an accepted `for i, v in xs` whose binding is never read used
// to emit Go that cannot build (`declared and not used`; and
// `no new variables on left side of :=` when neither binding is read).
// ---------------------------------------------------------------------------

fn range_src(isi: &str) -> String {
    format!(
        "fn main() {{\n    xs := []int{{1, 2, 3}}\n    t := 0\n    for i, v in xs {{\n        {}\n    }}\n    fmt.println(\"{{t}}\")\n}}\n",
        isi
    )
}

#[test]
fn range_binding_never_read_drops_to_blank() {
    let go = emit(&range_src("t = t + i"));
    assert!(
        go.contains("for i, _ := range xs {"),
        "unread second binding must be `_`:\n{}",
        go
    );
}

#[test]
fn range_binding_only_second_read_drops_first_to_blank() {
    let go = emit(&range_src("t = t + v"));
    assert!(
        go.contains("for _, v := range xs {"),
        "unread first binding must be `_`:\n{}",
        go
    );
}

#[test]
fn range_binding_both_read_stays_intact() {
    let go = emit(&range_src("t = t + i + v"));
    assert!(
        go.contains("for i, v := range xs {"),
        "both bindings read must be untouched:\n{}",
        go
    );
}

#[test]
fn range_binding_neither_read_drops_the_binding_list() {
    // `for _, _ := range xs` is still invalid Go, so both-unread has to drop
    // the binding list entirely.
    let go = emit("fn main() {\n    xs := []int{1, 2, 3}\n    for i, v in xs {\n        fmt.println(\"x\")\n    }\n}\n");
    assert!(
        go.contains("for range xs {"),
        "both bindings unread must drop the list:\n{}",
        go
    );
    assert!(!go.contains("_, _"), "must not emit `_, _`:\n{}", go);
}

#[test]
fn range_binding_mention_inside_a_closure_counts_as_read() {
    // A mention anywhere - a closure included - must suppress the `_`, since
    // only the conservative direction is safe.
    let go = emit("fn main() {\n    xs := []int{1, 2, 3}\n    f := fn() {\n        fmt.println(\"x\")\n    }\n    for i, v in xs {\n        f()\n        fmt.println(\"{v}\")\n    }\n}\n");
    assert!(
        go.contains("for _, v := range xs {"),
        "a closure mention is still a read:\n{}",
        go
    );
    assert!(
        go.contains("%v") && go.contains("for _, v := range xs {"),
        "the READ binding v must stay a real binding:\n{}",
        go
    );
}

// Loop-label scope (HYB-E3010 companion; issue: break/continue loop with no
// enclosing loop / match-arm label propagation)
// ---------------------------------------------------------------------------

#[test]
fn match_arm_labeled_break_labels_enclosing_loop() {
    // stmt_has_loop_break used to skip Stmt::Match, so a `break loop` inside
    // a match arm left the loop unlabeled and the emitted `break hyLoop`
    // failed go vet (invalid break label). The arm is not a new loop — the
    // enclosing loop must carry the hyLoop label.
    let go = emit(
        "fn main() {\n    o := some(1)\n    loop {\n        match o {\n            some(v) => if v > 0 { break loop }\n            none => {}\n        }\n    }\n}\n",
    );
    assert!(go.contains("hyLoop:"), "label missing:\n{}", go);
    assert!(go.contains("break hyLoop"), ":\n{}", go);
}

#[test]
fn match_arm_labeled_continue_labels_enclosing_while_for() {
    let go = emit(
        "fn main() {\n    o := some(1)\n    x := 1\n    for x > 0 {\n        match o {\n            some(v) => if v > 0 { continue loop }\n            none => {}\n        }\n    }\n}\n",
    );
    assert!(go.contains("hyLoop:"), "label missing:\n{}", go);
    assert!(go.contains("continue hyLoop"), ":\n{}", go);
}

#[test]
fn closure_labeled_break_without_closure_loop_refuses_emit() {
    // Labels do not cross func boundaries: a closure-nested `break loop`
    // with no loop INSIDE the literal cannot target the outer frame's label
    // (Go rejects it too). The checker's Tier-0 walk skips closure bodies,
    // so the emitter is the backstop — refuse instead of emitting
    // `break hyLoop` against an undefined label.
    let file = parse_file(
        "t.hy",
        "fn main() {\n    f := fn() {\n        break loop\n    }\n    f()\n}\n",
    )
    .expect("parse");
    match super::emit_go(&file) {
        Ok(g) => panic!("expected unsupported error, got:\n{}", g),
        Err(e) => assert!(
            e.message.contains("no enclosing loop"),
            "message should name the missing loop: {}",
            e.message
        ),
    }
}

#[test]
fn closure_with_own_loop_labeled_break_emits() {
    // Control: a closure that owns its loop labels itself and emits fine.
    let go = emit(
        "fn main() {\n    f := fn() {\n        loop {\n            break loop\n        }\n    }\n    f()\n}\n",
    );
    assert!(go.contains("hyLoop:"), "label missing:\n{}", go);
    assert!(go.contains("break hyLoop"), ":\n{}", go);
}

#[test]
fn e3_short_circuit_and_or_with_try_guards_rhs_evaluation() {
    let go = emit(
        "fn touched() (bool, error) {\n    return true, nil\n}\n\nfn main() {\n    a := false && touched()?\n    b := true || touched()?\n    fmt.println(a, b)\n}\n",
    );
    assert!(
        go.contains("hyT0 := false\n\t\tif hyT0 {\n\t\t\thyT1, hyErr2 := touched()"),
        "&& must guard RHS try evaluation under `if hyT0`:\n{}",
        go
    );
    assert!(
        go.contains("hyT3 := true\n\t\tif !hyT3 {\n\t\t\thyT4, hyErr5 := touched()"),
        "|| must guard RHS try evaluation under `if !hyT3`:\n{}",
        go
    );
    assert!(go.contains("a := hyT0"), "a binds hoisted short-circuit temp:\n{}", go);
    assert!(go.contains("b := hyT3"), "b binds hoisted short-circuit temp:\n{}", go);
}

#[test]
fn e3_if_init_with_try_in_cond_preserves_initializer_scope() {
    let go = emit(
        "fn check(x bool) (bool, error) {\n    return x, nil\n}\n\nfn main() {\n    if ok := false; ok && check(ok)? {\n        fmt.println(ok)\n    }\n}\n",
    );
    let init_at = go.find("ok := false").expect("init emitted");
    let cond_at = go.find("hyT0 := ok").expect("short-circuit temp uses ok");
    assert!(
        init_at < cond_at,
        "if-init binding must precede hoisted condition:\n{}",
        go
    );
}

#[test]
fn e3_hoist_preserves_preceding_operand_evaluation_order() {
    let go = emit(
        "fn first() int {\n    return 1\n}\n\nfn second() (int, error) {\n    return 2, nil\n}\n\nfn main() {\n    total := first() + second()?\n    fmt.println(total)\n}\n",
    );
    let first_at = go.find("hyT0 := first()").expect("first() hoisted first");
    let second_at = go
        .find("hyT1, hyErr2 := second()")
        .expect("second() hoisted after first()");
    assert!(
        first_at < second_at,
        "preceding side-effecting operand must evaluate before hoisted try:\n{}",
        go
    );
    assert!(
        go.contains("total := hyT0 + hyT1"),
        "substituted in order:\n{}",
        go
    );
}

#[test]
fn non_exhaustive_option_match_rejects_at_check_and_emits_no_garbage() {
    // AMENDMENT (maintainer, 2026-10-16, logged in the merge message):
    // this test previously pinned an EMITTER-side EmitError citing E5001.
    // pr-173 moved exhaustiveness into the CHECKER (HYB-E5001 fires at
    // hyb check; verified by hy-check's own suite), so the emitter's
    // no-some-arm path intentionally lowers nothing. What must never come
    // back is #143 R1/R2's failure mode: an orphan `} else {` or a silent
    // single-arm branch in the emitted Go. These pins stay:
    let go_none_only = emit(
        "fn main() {\n    var x option[int] = some(1)\n    match x {\n        none: fmt.println(\"none\")\n    }\n}\n",
    );
    assert!(!go_none_only.contains("} else {"), "orphan else came back:\n{}", go_none_only);

    let go_some_only = emit(
        "fn main() {\n    var x option[int] = some(1)\n    match x {\n        some(v): fmt.println(v)\n    }\n}\n",
    );
    assert!(!go_some_only.contains("} else {"), "orphan else came back:\n{}", go_some_only);
}
