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

#[test]
fn bare_error_fn_under_try_emits_zero_value_temps() {
    // #726: `fn step() error` returns ONE Go value. The fn shape used to count
    // the trailing error as a value too, so `step()?` emitted
    // `hyT0, hyErr1 := step()` against `func step() error` — go build failed
    // with "assignment mismatch: 2 variables but step returns 1 value".
    let go = emit(
        &"fn step() error {\n    return nil\n}\n\nfn run() error {\n    step()?\n    return nil\n}",
    );
    assert!(
        go.contains("hyErr0 := step()"),
        "bare-error try must emit a single error temp, got:\n{}",
        go
    );
    assert!(
        !go.contains("hyT0, hyErr0 := step()"),
        "no value temp may be declared for a bare-error result:\n{}",
        go
    );
}

    #[test]
    fn defer_call_argument_try_hoists_before_the_defer() {
        // P1 (§7): a deferred call's arguments are evaluated at defer time, so a
        // `?` inside them hoists to a binding BEFORE the `defer` — the call
        // itself stays deferred. Regression for the check→emit gap where the
        // hoister never registered the temp and `emit_expr` refused the `?` as
        // "outside a binding position" on a program the checker accepted.
        let go = emit(
            "fn probe() (n int, e error) {\n    return 1, nil\n}\n\nfn sink(n int) {\n    fmt.println(n)\n}\n\nfn main() (code int) {\n    defer sink(probe()?)\n    return 0\n}\n",
        );
        let hoist = go
            .find(":= probe()")
            .unwrap_or_else(|| panic!("argument `?` must hoist://n{}", go));
        let defer_at = go
            .find("defer sink(")
            .unwrap_or_else(|| panic!("defer stays a deferred call://n{}", go));
        assert!(hoist < defer_at, "hoist must precede the defer://n{}", go);
    }

    #[test]
    fn defer_parenthesized_operand_emits_without_parens() {
        // Go rejects a parenthesized defer operand ("expression in defer must be
        // function call"), so a parenthesized call must lower to a bare one:
        // `defer (fmt.println("done"))` -> `defer fmt.Println("done")`.
        let go = emit("fn main() {\n    defer (fmt.println(\"done\"))\n}\n");
        assert!(go.contains("defer fmt.Println(\"done\")"), ":\n{}", go);
        assert!(
            !go.contains("defer ("),
            "the defer operand must not keep its parens://n{}",
            go
        );
    }

    #[test]
    fn defer_parenthesized_call_argument_try_hoists_before_the_defer() {
        // Regression (adversarial review): `defer (sink(probe()?))` — the operand
        // is a `Paren{Call}`, so the hoist gate has to unwrap parens exactly like
        // the checker's HYB-E2001 test does (a parenthesized call is still a
        // call). Otherwise the checker accepts the program and the emitter dies
        // late with "`?` outside a binding position" — the very gap this branch
        // closes, reintroduced by one set of parens.
        let go = emit(
            "fn probe() (n int, e error) {\n    return 1, nil\n}\n\nfn sink(n int) {\n    fmt.println(n)\n}\n\nfn main() (code int) {\n    defer (sink(probe()?))\n    return 0\n}\n",
        );
        let hoist = go
            .find(":= probe()")
            .unwrap_or_else(|| panic!("parenthesized argument `?` must hoist://n{}", go));
        let defer_at = go
            .find("defer sink(")
            .unwrap_or_else(|| panic!("defer lowered without parens://n{}", go));
        assert!(hoist < defer_at, "hoist must precede the defer://n{}", go);
        assert!(
            !go.contains("defer ("),
            "no parens in the emitted defer://n{}",
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
fn mixed_results_on_plain_fn_are_uniformly_named() {
    let go = emit("fn f() (code int, string) {\n    return 3, \"ok\"\n}\n");
    assert!(
        go.contains("func f() (code int, hyR1 string) {"),
        "named-then-unnamed results must remain buildable:\n{}",
        go
    );

    let go = emit("fn f() (int, text string) {\n    return 3, \"ok\"\n}\n");
    assert!(
        go.contains("func f() (hyR0 int, text string) {"),
        "unnamed-then-named results must retain their types:\n{}",
        go
    );
}

#[test]
fn mixed_results_on_fn_literal_are_uniformly_named() {
    let go = emit(
        "fn main() {\n    f := fn() (code int, string) {\n        return 3, \"ok\"\n    }\n}\n",
    );
    assert!(
        go.contains("func() (code int, hyR1 string) {"),
        "function literals use the same Go result-list rules:\n{}",
        go
    );
}

#[test]
fn fully_unnamed_results_keep_their_compact_emission() {
    let go = emit("fn f() (int, string) {\n    return 3, \"ok\"\n}\n");
    assert!(go.contains("func f() (int, string) {"), ":\n{}", go);
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
    assert!(
        go.contains("shared counter under a lock"),
        "reason in marker:\n{}",
        go
    );
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
        "fn main() {\n    select {\n        v, ok := <-sq:\n            _ = v\n            _ = ok\n        <-deadline:\n            _ = deadline\n    }\n}\n",
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
    // Inclusive ranges use a continue-safe stop flag (#322), not
    // `i <= 8; i++` (that form wraps at a type maximum). Bounds are
    // hoisted so the stop check is `i != hyHiN`.
    assert!(
        go.contains("<= ") && go.contains("8"),
        "inclusive empty-range guard:\n{}",
        go
    );
    assert!(
        go.contains("i != "),
        "inclusive last-value stop check:\n{}",
        go
    );
    assert!(
        !go.contains("for i := 1; i <= 8; i++ {"),
        "must not emit the wrapping post-increment form:\n{}",
        go
    );
}

#[test]
fn inclusive_range_at_type_maximum_uses_stop_flag() {
    // Issue #322: int8 127..=127 must not lower to `value <= high; value++`
    // (increment wraps to -128 and the loop never ends).
    let go = emit(
        "fn main() {\n    var high int8 = 127\n    for value in high..=high {\n        fmt.println(value)\n    }\n}\n",
    );
    assert!(
        !go.contains("value <= high; value++")
            && !go.contains("for value := high; value <= high; value++"),
        "wrapping inclusive form must not be emitted:\n{}",
        go
    );
    assert!(
        go.contains("value !="),
        "continue-safe stop check present:\n{}",
        go
    );
    assert!(
        go.contains("<= "),
        "empty-range guard still present:\n{}",
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
        "fn main() {\n    loop {\n        break loop\n    }\n    loop {\n        break loop\n    }\n}\n",
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
        "fn main() {\n    loop {\n        loop {\n            break loop\n        }\n        break loop\n    }\n}\n",
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
fn generic_struct_and_fn_infer_missing_constraints_722() {
    // Issue #722: `struct box[t] { v t }` without an explicit constraint must
    // emit `type box[t any] struct {` so `gofmt` does not rewrite `box[t] struct`
    // into the invalid array type `type box [t]struct {`.
    let go = emit(
        "struct box[t] {\n    v t\n}\n\nstruct table[k, v] {\n    m map[k]v\n}\n\nfn id[t](x t) t {\n    return x\n}\n\nfn main() {\n    b := box[u8]{ v: 3 }\n    fmt.println(id[u8](b.v))\n}\n",
    );
    assert!(
        go.contains("type box[t any] struct {"),
        "expected `type box[t any] struct {{` in:\n{}",
        go
    );
    assert!(
        go.contains("type table[k comparable, v any] struct {"),
        "expected `k comparable, v any` in:\n{}",
        go
    );
    assert!(
        go.contains("func id[t any](x t) t {"),
        "expected `func id[t any](x t) t {{` in:\n{}",
        go
    );
    assert!(
        go.contains("box[uint8]{v: 3}"),
        "expected SPEC primitive type arg `u8` to lower to `uint8` in:\n{}",
        go
    );
    assert!(
        go.contains("id[uint8](b.v)"),
        "expected generic fn call type arg `u8` to lower to `uint8` in:\n{}",
        go
    );
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
fn match_unguarded_some_first_emits_buildable_go() {
    // An UNGUARDED `some` arm placed before another `some` arm matches
    // whenever `ok` is true, so later arms are unreachable. The emitter
    // must not chain a `} else {` onto a chain that has no preceding `if`
    // (that produced an orphan `else` and unbuildable Go, while `hyb check`
    // reported 0 diagnostics).
    let go = emit(
        "fn main() {\n    opt := some(3)\n    match opt {\n        some(v): {\n            fmt.println(\"small\")\n        }\n        some(v) if v > 10: {\n            fmt.println(\"big\")\n        }\n        none: {\n            fmt.println(\"none\")\n        }\n    }\n}\n",
    );
    let opens = go.matches('{').count();
    let closes = go.matches('}').count();
    assert_eq!(opens, closes, "unbalanced braces (orphan else):\n{}", go);
    assert_eq!(
        go.matches("} else {").count(),
        1,
        "expected exactly one else (the none arm):\n{}",
        go
    );
    let ok = go.find("if hyMatch0.ok {").expect("ok branch");
    let small = go.find("fmt.Println(\"small\")").expect("small arm body");
    let els = go.find("} else {").expect("none arm");
    assert!(
        ok < small && small < els,
        "unguarded arm body must be inside the ok branch, before the else:\n{}",
        go
    );
}

#[test]
fn match_guarded_then_unguarded_some_still_chains() {
    // Guarded-first ordering must keep working: the guarded arm is an `if`
    // and the first unguarded arm is its `else`.
    let go = emit(
        "fn main() {\n    opt := some(15)\n    match opt {\n        some(v) if v > 10: {\n            fmt.println(\"big\")\n        }\n        some(v): {\n            fmt.println(\"small\")\n        }\n        none: {\n            fmt.println(\"none\")\n        }\n    }\n}\n",
    );
    assert_eq!(
        go.matches('{').count(),
        go.matches('}').count(),
        ":\n{}",
        go
    );
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
fn assign_slot_loan_into_pointer_var_points_at_copy() {
    // A pointer-typed target cannot hold the copied element itself
    // (`cannot use g.nodes[from] (variable of struct type node) as *node`):
    // it points at a fresh copy, and the store-back dereferences it.
    let go = emit(
        "struct node {\n    edges []int\n}\n\nstruct graph {\n    nodes map[int]node\n}\n\nfn (g *graph) connect(from int, to int) {\n    var n *node\n    n = &g.nodes[from]\n    n.edges = append(n.edges, to)\n}\n",
    );
    assert!(
        go.contains("hyLoan0 := g.nodes[from]"),
        "copy into a temp:\n{}",
        go
    );
    assert!(go.contains("n = &hyLoan0"), "pointer at the copy:\n{}", go);
    assert!(
        !go.contains("n = g.nodes[from]"),
        "value copy into a pointer var cannot build:\n{}",
        go
    );
    assert!(
        go.contains("g.nodes[from] = *n"),
        "dereferenced store-back:\n{}",
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
fn single_var_iterate_over_chan_typed_var_binds_value() {
    let go = emit("fn f() {\n    var ch chan int\n    for v in ch {\n        _ = v\n    }\n}\n");
    assert!(
        go.contains("for v := range ch {"),
        "typed chan var direct range:\n{}",
        go
    );
}

#[test]
fn single_var_iterate_over_chan_call_binds_value() {
    let go = emit("fn get_chan() chan int {\n    return make(chan int, 1)\n}\n\nfn f() {\n    for v in get_chan() {\n        _ = v\n    }\n}\n");
    assert!(
        go.contains("for v := range get_chan() {"),
        "channel call range binds received value:\n{}",
        go
    );
}

#[test]
fn single_var_iterate_over_chan_paren_binds_value() {
    let go = emit("fn f(ch chan int) {\n    for v in (ch) {\n        _ = v\n    }\n}\n");
    assert!(
        go.contains("for v := range (ch) {") || go.contains("for v := range ch {"),
        "channel paren range binds received value:\n{}",
        go
    );
}

#[test]
fn single_var_iterate_over_chan_make_call_binds_value() {
    let go = emit("fn f() {\n    for v in make(chan int, 2) {\n        _ = v\n    }\n}\n");
    assert!(
        go.contains("for v := range make(chan int, 2) {"),
        "inline make chan range binds received value:\n{}",
        go
    );
}

#[test]
fn two_var_iterate_over_chan_is_rejected() {
    let err = emit_err("fn f(ch chan int) {\n    for a, b in ch {\n        _ = a\n        _ = b\n    }\n}\n");
    assert!(
        err.contains("cannot range over channel with two iteration variables"),
        "expected two-var chan range rejection, got:\n{}",
        err
    );
}

#[test]
fn single_var_iterate_over_make_slice_binds_element() {
    let go = emit("fn f() {\n    for x in make([]int, 5) {\n        _ = x\n    }\n}\n");
    assert!(
        go.contains("for _, x := range make([]int, 5) {"),
        "make slice range must bind element (for _, x := range ...):\n{}",
        go
    );
    assert!(
        !go.contains("for x := range make([]int, 5) {"),
        "make slice range must not be lowered as channel range:\n{}",
        go
    );
}

#[test]
fn single_var_iterate_over_make_slice_variable_binds_element() {
    let go = emit("fn f() {\n    s := make([]int, 5)\n    for x in s {\n        _ = x\n    }\n}\n");
    assert!(
        go.contains("for _, x := range s {"),
        "make slice variable range must bind element (for _, x := range s):\n{}",
        go
    );
    assert!(
        !go.contains("for x := range s {"),
        "make slice variable must not be recorded as channel binding:\n{}",
        go
    );
}

#[test]
fn single_var_iterate_over_map_variable_does_not_lower_as_channel() {
    let go = emit("fn f() {\n    m := map[string]int{}\n    for k in m {\n        _ = k\n    }\n}\n");
    assert!(
        go.contains("for _, k := range m {"),
        "map variable must not be lowered as channel:\n{}",
        go
    );
    assert!(
        !go.contains("for k := range m {"),
        "map variable single-var range must not be lowered as channel range:\n{}",
        go
    );
}

#[test]
fn single_var_iterate_over_make_map_literal_does_not_lower_as_channel() {
    let go = emit("fn f() {\n    m := make(map[string]int{})\n    for k in m {\n        _ = k\n    }\n}\n");
    assert!(
        go.contains("for _, k := range m {"),
        "make(map) variable must not be lowered as channel:\n{}",
        go
    );
}


#[test]
fn two_var_iterate_over_make_slice_allowed() {
    let go = emit("fn f() {\n    for i, x in make([]int, 5) {\n        _ = i\n        _ = x\n    }\n}\n");
    assert!(
        go.contains("for i, x := range make([]int, 5) {"),
        "two-var range over make slice must be allowed:\n{}",
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

#[test]
fn slice_pop_prelude_panics_with_hy_panic_on_empty() {
    // T7 pins `.pop()` as "panics on empty", and `option.unwrap()` already
    // brands its panic (`HY-PANIC/unwrap-none`), so an empty pop must report
    // the Hybrid operation instead of Go's raw `index out of range [-1]`,
    // which points at generated code (issue #2442).
    let go =
        emit("type node_id = int\n\nfn f(stack []node_id) node_id {\n    return stack.pop()\n}\n");
    assert!(
        go.contains("\tif n == 0 {\n\t\tpanic(\"HY-PANIC/pop-empty\")\n\t}"),
        "pop prelude must guard the empty case with the branded panic:\n{}",
        go
    );
    // Semantics unchanged: the panic still happens, and the non-empty path is
    // byte-for-byte what it was.
    assert!(go.contains("v := (*s)[n-1]"), "pop reads the last element:\n{}", go);
    assert!(go.contains("*s = (*s)[:n-1]"), "pop shrinks the slice:\n{}", go);
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
fn diff_elided_map_value_struct_literal_under_parens() {
    // Same elision, one redundant paren deeper: `({replicas: 2, tag: "prod"})`
    // is the same expression, so the expected TYPE must still be applied.
    // Before the peel this fell through to the "spell the type" refusal — a
    // check-accepts / emit-rejects gap.
    let go = emit(
        "struct service {\n    replicas int\n    tag string\n}\n\nfn main() {\n    e := env{services: map[string]service{\n        \"frontend\": ({replicas: 2, tag: \"prod\"}),\n    }}\n    _ = e\n}\n\nstruct env {\n    services map[string]service\n}",
    );
    assert!(
        go.contains("\"frontend\": service{replicas: 2, tag: \"prod\"}"),
        "elided head filled through parens:\n{}",
        go
    );
}

#[test]
fn diff_elided_slice_elem_struct_literal_under_parens() {
    let go = emit(
        "struct service {\n    replicas int\n}\n\nfn main() {\n    s := []service{({replicas: 2}), (({replicas: 3}))}\n    _ = s\n}\n",
    );
    assert!(
        go.contains("[]service{service{replicas: 2}, service{replicas: 3}}"),
        "elided heads filled through (nested) parens:\n{}",
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
    let go = emit(
        "fn main() {\n    const thresholds = [10, 50]\n    fmt.println(\"{thresholds[0]}\")\n}",
    );
    assert!(go.contains("var thresholds = []int{10, 50}"), ":\n{}", go);
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
fn pointer_param_forwarded_to_pointer_param_not_double_addressed() {
    // A parameter already having a pointer type (*env) must NOT be auto-addressed
    // with '&' when passed to a pointer parameter (*env), which would produce **env.
    let go = emit(
        "struct env {\n    n int\n}\n\nfn scale(e *env) {\n    _ = e\n}\n\nfn forward(p *env) {\n    scale(p)\n}\n",
    );
    assert!(go.contains("scale(p)"), ":\n{}", go);
    assert!(
        !go.contains("scale(&p)"),
        "must not double-address pointer param:\n{}",
        go
    );
}

#[test]
fn pointer_var_passed_to_pointer_param_not_double_addressed() {
    // A variable declared with a pointer type (var p *env) must NOT be auto-addressed
    // when passed to a pointer parameter.
    let go = emit(
        "struct env {\n    n int\n}\n\nfn scale(e *env) {\n    _ = e\n}\n\nfn main() {\n    var p *env\n    scale(p)\n}\n",
    );
    assert!(go.contains("scale(p)"), ":\n{}", go);
    assert!(
        !go.contains("scale(&p)"),
        "must not double-address pointer var:\n{}",
        go
    );
}

#[test]
fn pointer_binding_from_addrof_not_double_addressed() {
    // A binding initialized from an address-of expression (p := &e) is already a
    // pointer and must not be auto-addressed when passed to a pointer parameter.
    let go = emit(
        "struct env {\n    n int\n}\n\nfn scale(e *env) {\n    _ = e\n}\n\nfn main() {\n    e := env{}\n    p := &e\n    scale(p)\n}\n",
    );
    assert!(go.contains("p := &e"), ":\n{}", go);
    assert!(go.contains("scale(p)"), ":\n{}", go);
    assert!(
        !go.contains("scale(&p)"),
        "must not double-address pointer local:\n{}",
        go
    );
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

#[test]
fn single_range_binding_never_read_drops_to_for_range() {
    let go = emit("fn main() {\n    xs := []int{1, 2, 3}\n    for x in xs {\n        fmt.println(\"item\")\n    }\n}\n");
    assert!(
        go.contains("for range xs {"),
        "unread single binding must lower to `for range xs`:\n{}",
        go
    );
    assert!(!go.contains("for _, x :="), "must not declare unread variable:\n{}", go);
}

#[test]
fn single_range_binding_over_chan_never_read_drops_to_for_range() {
    let go = emit("fn worker(jobs <-chan int) {\n    for j in jobs {\n        fmt.println(\"item\")\n    }\n}\n");
    assert!(
        go.contains("for range jobs {"),
        "unread single channel binding must lower to `for range jobs`:\n{}",
        go
    );
    assert!(!go.contains("for j :="), "must not declare unread variable:\n{}", go);
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
fn match_arm_labeled_continue_labels_enclosing_loop() {
    let go = emit(
        "fn main() {\n    o := some(1)\n    x := 1\n    loop {\n        for x > 0 {\n            match o {\n                some(v) => if v > 0 { continue loop }\n                none => {}\n            }\n        }\n    }\n}\n",
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
    assert!(
        go.contains("a := hyT0"),
        "a binds hoisted short-circuit temp:\n{}",
        go
    );
    assert!(
        go.contains("b := hyT3"),
        "b binds hoisted short-circuit temp:\n{}",
        go
    );
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
    assert!(
        !go_none_only.contains("} else {"),
        "orphan else came back:\n{}",
        go_none_only
    );

    let go_some_only = emit(
        "fn main() {\n    var x option[int] = some(1)\n    match x {\n        some(v): fmt.println(v)\n    }\n}\n",
    );
    assert!(
        !go_some_only.contains("} else {"),
        "orphan else came back:\n{}",
        go_some_only
    );
}

#[test]
fn user_struct_shadowing_primitive_zero_value_issue_326() {
    let src = r#"
struct rune {
    count int
}
fn fail_helper() (int, error) {
    return 0, nil
}
fn test_fn() (rune, error) {
    fail_helper()?
    return rune{ count: 1 }, nil
}
"#;
    let go = emit(src);
    assert!(
        go.contains("return *new(rune), hyErr1"),
        "expected return *new(rune), hyErr1, got:\n{}",
        go
    );
}

#[test]
fn select_recv_single_var_unread_drops_to_bare_receive() {
    let go = emit("fn main() {\n    select {\n        v := <-ch:\n            fmt.println(\"tick\")\n    }\n}\n");
    assert!(
        go.contains("case <-ch:"),
        "unread single binding must lower to bare receive:\n{}",
        go
    );
    assert!(
        !go.contains("case v"),
        "must not declare unread variable in case:\n{}",
        go
    );
    assert!(
        !go.contains(":="),
        "must not use := with no new variables:\n{}",
        go
    );
}

#[test]
fn select_recv_single_var_blank_drops_to_bare_receive() {
    let go = emit("fn main() {\n    select {\n        _ := <-ch:\n            fmt.println(\"tick\")\n    }\n}\n");
    assert!(
        go.contains("case <-ch:"),
        "blank single binding must lower to bare receive:\n{}",
        go
    );
    assert!(
        !go.contains(":="),
        "must not use := with blank variable:\n{}",
        go
    );
}

#[test]
fn select_recv_two_var_neither_read_drops_to_bare_receive() {
    let go = emit("fn main() {\n    select {\n        v, ok := <-ch:\n            fmt.println(\"tick\")\n    }\n}\n");
    assert!(
        go.contains("case <-ch:"),
        "two unread bindings must drop to bare receive:\n{}",
        go
    );
    assert!(
        !go.contains("case v"),
        "must not declare unread variables in case:\n{}",
        go
    );
    assert!(
        !go.contains(":="),
        "must not use := with no new variables:\n{}",
        go
    );
}

#[test]
fn select_recv_two_var_neither_read_explicit_blank() {
    let go = emit("fn main() {\n    select {\n        _, _ := <-ch:\n            fmt.println(\"tick\")\n    }\n}\n");
    assert!(
        go.contains("case <-ch:"),
        "two blank bindings must drop to bare receive:\n{}",
        go
    );
    assert!(
        !go.contains(":="),
        "must not emit invalid `case _, _ := <-ch:`:\n{}",
        go
    );
}

#[test]
fn select_recv_two_var_first_only_read_blanks_second() {
    let go = emit("fn main() {\n    select {\n        v, ok := <-ch:\n            fmt.println(\"{v}\")\n    }\n}\n");
    assert!(
        go.contains("case v, _ := <-ch:"),
        "unread second binding must be blanked:\n{}",
        go
    );
}

#[test]
fn select_recv_two_var_second_only_read_blanks_first() {
    let go = emit("fn main() {\n    select {\n        v, ok := <-ch:\n            if ok {\n                fmt.println(\"open\")\n            }\n    }\n}\n");
    assert!(
        go.contains("case _, ok := <-ch:"),
        "unread first binding must be blanked:\n{}",
        go
    );
}

#[test]
fn select_recv_two_var_both_read_kept_intact() {
    let go = emit("fn main() {\n    select {\n        v, ok := <-ch:\n            if ok {\n                fmt.println(\"{v}\")\n            }\n    }\n}\n");
    assert!(
        go.contains("case v, ok := <-ch:"),
        "both read bindings must remain intact:\n{}",
        go
    );
}

#[test]
fn explicit_multi_segment_go_import_emits_full_path() {
    let go = emit("import \"crypto/sha256\"\n\nfn main() {\n    _ = sha256.new()\n}\n");
    assert!(
        go.contains("\"crypto/sha256\""),
        "must emit full package path, got:\n{}",
        go
    );
    assert!(
        !go.contains("\n\t\"sha256\"\n"),
        "must not strip directory prefix to unresolvable root:\n{}",
        go
    );
}

#[test]
fn explicit_nested_stdlib_and_third_party_imports_preserved() {
    let go = emit("import \"encoding/json\"\nimport \"net/url\"\n\nfn main() {\n    _ = json.marshal(\"hi\")\n    _ = url.parse(\"http://example.com\")\n}\n");
    assert!(
        go.contains("\"encoding/json\""),
        "must preserve encoding/json import path:\n{}",
        go
    );
    assert!(
        go.contains("\"net/url\""),
        "must preserve net/url import path:\n{}",
        go
    );
    assert!(
        !go.contains("\n\t\"json\"\n") && !go.contains("\n\t\"url\"\n"),
        "must not emit naked stripped roots:\n{}",
        go
    );
}

#[test]
fn bare_return_in_void_fn_has_no_trailing_whitespace() {
    let go = emit("fn main() {\n    return\n}\n");
    assert!(go.contains("\treturn\n"), "must emit clean return without trailing space:\n{}", go);
    assert!(!go.contains("\treturn \n"), "must not emit trailing whitespace:\n{}", go);
}

#[test]
fn return_blank_in_error_slot_lowers_to_nil() {
    let go = emit("fn compute() (int, error) {\n    return 42, _\n}\n");
    assert!(
        go.contains("return 42, nil"),
        "return 42, _ must lower error slot to nil:\n{}",
        go
    );
    assert!(
        !go.contains("return 42, \n"),
        "must not emit empty slot with trailing comma:\n{}",
        go
    );
}

#[test]
fn return_all_blanks_lowers_zero_and_nil() {
    let go = emit("fn compute() (int, error) {\n    return _, _\n}\n");
    assert!(
        go.contains("return 0, nil"),
        "return _, _ must lower to zero value and nil:\n{}",
        go
    );
}

#[test]
fn slot_loan_flushes_at_nonlexical_last_use_before_same_block_read_delete_or_move() {
    let go = emit(
        "struct service {\n    replicas int\n}\n\nfn worker(m map[string]service, done chan<- bool) {\n    done <- true\n}\n\nfn main() {\n    m := map[string]service{\"api\": {replicas: 1}}\n    s := &m[\"api\"]\n    s.replicas = 5\n    fmt.println(m[\"api\"].replicas)\n    delete(m, \"api\")\n    done := make(chan bool, 1)\n    task worker(move m, done)\n    <-done\n}\n",
    );
    let mut_at = go.find("s.replicas = 5").expect("loan mutation");
    let flush_at = go.find("m[\"api\"] = s").expect("store-back emitted");
    let read_at = go
        .find("fmt.Println(m[\"api\"].replicas)")
        .expect("subsequent read emitted");
    let del_at = go.find("delete(m, \"api\")").expect("delete emitted");
    let spawn_at = go.find("go worker(m, done)").expect("task spawn emitted");
    assert!(
        mut_at < flush_at && flush_at < read_at && read_at < del_at && del_at < spawn_at,
        "slot loan must flush right after non-lexical last_use before same-block read/delete/move:\n{}",
        go
    );
    assert_eq!(
        go.matches("m[\"api\"] = s").count(),
        1,
        "must not emit duplicate store-back at block end:\n{}",
        go
    );
}


#[test]
fn slot_loan_rebind_flushes_previous_loan_before_overwriting() {
    let go = emit(
        "struct service {\n    replicas int\n}\n\nfn main() {\n    m1 := map[string]service{\"a\": {replicas: 1}}\n    m2 := map[string]service{\"b\": {replicas: 2}}\n    s := &m1[\"a\"]\n    s.replicas = 10\n    s = &m2[\"b\"]\n    s.replicas = 20\n}\n",
    );
    let mut_a = go.find("s.replicas = 10").expect("first mutation");
    let flush_a = go.find("m1[\"a\"] = s").expect("first store-back");
    let rebind_b = go.find("s = m2[\"b\"]").expect("rebind to m2");
    let mut_b = go.find("s.replicas = 20").expect("second mutation");
    let flush_b = go.find("m2[\"b\"] = s").expect("second store-back");
    assert!(
        mut_a < flush_a && flush_a < rebind_b && rebind_b < mut_b && mut_b < flush_b,
        "rebind must flush m1[\"a\"] = s before overwriting s with m2[\"b\"]:\n{}",
        go
    );
    assert_eq!(
        go.matches("m1[\"a\"] = s").count(),
        1,
        "m1[\"a\"] flushed once:\n{}",
        go
    );
}


#[test]
fn slot_loan_snapshots_mutated_or_nonidempotent_index_key() {
    let go = emit(
        "struct service {\n    replicas int\n}\n\nfn next_key() string {\n    return \"a\"\n}\n\nfn main() {\n    m := map[string]service{\"a\": {replicas: 1}, \"b\": {replicas: 2}}\n    k := \"a\"\n    s := &m[k]\n    k = \"b\"\n    s.replicas = 10\n    m2 := map[string]service{\"a\": {replicas: 1}}\n    s2 := &m2[next_key()]\n    s2.replicas = 30\n}\n",
    );
    assert!(
        go.contains("hyKey0 := k")
            && go.contains("s := m[hyKey0]")
            && go.contains("m[hyKey0] = s"),
        "mutated index variable k must be snapshotted into hyKey0:\n{}",
        go
    );
    assert!(
        go.contains("hyKey1 := next_key()")
            && go.contains("s2 := m2[hyKey1]")
            && go.contains("m2[hyKey1] = s2"),
        "call index expression next_key() must be evaluated once via hyKey1:\n{}",
        go
    );
}


#[test]
fn slot_loan_in_match_some_arm_flushes_inside_arm_scope() {
    let go = emit(
        "struct service {\n    replicas int\n}\n\nfn maybe_key() option[string] {\n    return some(\"a\")\n}\n\nfn main() {\n    m := map[string]service{\"a\": {replicas: 1}}\n    match maybe_key() {\n        some(k): {\n            s := &m[k]\n            s.replicas = 10\n        }\n        none: {\n        }\n    }\n}\n",
    );
    let ok_branch = go.find("if hyMatch0.ok {").expect("some arm branch");
    let flush_at = go.find("m[k] = s").expect("store-back in some arm");
    let else_branch = go.find("} else {").expect("else branch");
    assert!(
        ok_branch < flush_at && flush_at < else_branch,
        "some(k) arm slot loan must flush inside `if hyMatch0.ok` before `}} else {{`:\n{}",
        go
    );
}


#[test]
fn slot_loan_parallel_define_assign_and_try_in_place() {
    let go = emit(
        "struct service {\n    replicas int\n}\n\nfn get_key() (string, error) {\n    return \"a\", nil\n}\n\nfn main() {\n    m1 := map[string]service{\"a\": {replicas: 1}}\n    m2 := map[string]service{\"b\": {replicas: 2}}\n    s1, s2 := &m1[get_key()?], &m2[\"b\"]\n    s1.replicas = 10\n    s2.replicas = 20\n}\n",
    );
    assert!(
        !go.contains("&m1") && !go.contains("&m2"),
        "parallel slot loans with `?` must strip `&` and lower to copy-read + store-back:\n{}",
        go
    );
    assert!(
        go.contains("s1.replicas = 10") && go.contains("m2[\"b\"] = s2"),
        "parallel slot loans must emit store-backs for both bindings:\n{}",
        go
    );
}


#[test]
fn task_preserves_variadic_spread() {
    let go = emit(
        "fn worker(items []string) {}\n\nfn main() {\n    items := []string{\"a\", \"b\"}\n    task worker(items...)\n}\n",
    );
    assert!(
        go.contains("go worker(items...)"),
        "task must preserve variadic spread `...`:\n{}",
        go
    );
}

#[test]
fn task_auto_addresses_pointer_params() {
    let go = emit(
        "struct env {\n    name string\n}\n\nfn worker(e *env) {}\n\nfn main() {\n    e := env{name: \"prod\"}\n    task worker(e)\n}\n",
    );
    assert!(
        go.contains("go worker(&e)"),
        "task must auto-address addressable pointer argument:\n{}",
        go
    );
}

#[test]
fn task_preserves_already_addressed_pointer_param() {
    let go = emit(
        "struct env {\n    name string\n}\n\nfn worker(e *env) {}\n\nfn main() {\n    e := env{name: \"prod\"}\n    task worker(&e)\n}\n",
    );
    assert!(
        go.contains("go worker(&e)"),
        "task must not double-address:\n{}",
        go
    );
}

#[test]
fn task_package_call_uses_package_and_normalizes_name() {
    let go = emit(
        "fn main() {\n    task time.sleep(100)\n}\n",
    );
    assert!(
        go.contains("\"time\""),
        "package must be imported:\n{}",
        go
    );
    assert!(
        go.contains("go time.Sleep(100)"),
        "method name must normalize and emit go:\n{}",
        go
    );
}

#[test]
fn task_plain_call_still_emits_verbatim() {
    let go = emit(
        "fn worker(x int) {}\n\nfn main() {\n    x := 42\n    task worker(x)\n}\n",
    );
    assert!(
        go.contains("go worker(x)"),
        "plain task call must emit verbatim:\n{}",
        go
    );
}

#[test]
fn task_with_try_args_hoists_in_order_before_spawn() {
    let go = emit(
        "fn worker(a int, b int) {}\nfn first() (int, error) { return 1, nil }\nfn second() (int, error) { return 2, nil }\nfn run() error {\n    task worker(first()?, second()?)\n    return nil\n}\n",
    );
    let first_at = go.find("first()").expect("first() present");
    let second_at = go.find("second()").expect("second() present");
    let spawn_at = go.find("go worker(").expect("go worker spawn present");
    assert!(first_at < second_at, "first try must evaluate before second try");
    assert!(second_at < spawn_at, "tries must evaluate before goroutine spawn");
    assert!(go.contains("go worker(hyT0, hyT2)"), "arguments substituted with temps:\n{}", go);
}
// ---------------------------------------------------------------------------
// #547 — a shadowing binder must not stretch a slot loan's live range.
//
// `n := &m["a"]` opens an O8 slot loan on `n`; the non-lexical flush emits the
// store-back (`m["a"] = n`) as soon as no later statement reads `n`. A `for` /
// `select` binder that RE-DECLARES `n` makes the body's `n` a different
// binding, so those reads must not keep the loan alive. Before the fix the
// store-back was deferred past `task worker(move m, done)` and raced with the
// worker on the moved map (`go run -race`: DATA RACE, 3/3 runs).
// ---------------------------------------------------------------------------

/// The #547 report verbatim: the range binder reuses the loan's name, and the
/// store-back must land BEFORE the spawn that moves the map.
#[test]
fn slot_loan_flushes_before_move_when_range_binder_shadows_the_loan_name_issue_547() {
    let go = emit(
        "struct item {\n    x int\n}\n\nfn worker(m map[string]item, done chan bool) {\n    for i in 0..1000 {\n        m[\"w\"] = item{x: i}\n    }\n    done <- true\n}\n\nfn main() {\n    m := map[string]item{\"a\": item{x: 0}}\n    done := make(chan bool, 1)\n    n := &m[\"a\"]\n    n.x = 1\n    task worker(move m, done)\n    total := 0\n    for n in 0..1000 {\n        total += n\n    }\n    <-done\n    fmt.println(total)\n}\n",
    );
    let mut_at = go.find("n.x = 1").expect("loan mutation");
    let flush_at = go.find("m[\"a\"] = n").expect("store-back emitted");
    let spawn_at = go.find("go worker(m, done)").expect("task spawn emitted");
    assert!(
        mut_at < flush_at && flush_at < spawn_at,
        "a range binder that shadows the loan name must not defer the store-back past the spawn (#547):\n{}",
        go
    );
    assert_eq!(
        go.matches("m[\"a\"] = n").count(),
        1,
        "store-back must be emitted exactly once:/n{}",
        go
    );
}

/// `ForKind::Iterate` binder (`for n in items`) — same class as the range case.
#[test]
fn slot_loan_flushes_before_move_when_iterate_binder_shadows_the_loan_name() {
    let go = emit(
        "struct service {\n    replicas int\n}\n\nfn worker(m map[string]service, done chan bool) {\n    done <- true\n}\n\nfn main() {\n    m := map[string]service{\"a\": {replicas: 1}}\n    n := &m[\"a\"]\n    n.replicas = 5\n    done := make(chan bool, 1)\n    task worker(move m, done)\n    items := []int{1, 2, 3}\n    total := 0\n    for n in items {\n        total += n\n    }\n    <-done\n    fmt.println(total)\n}\n",
    );
    let flush_at = go.find("m[\"a\"] = n").expect("store-back emitted");
    let spawn_at = go.find("go worker(m, done)").expect("task spawn emitted");
    assert!(
        flush_at < spawn_at,
        "an iterate binder that shadows the loan name must not defer the store-back past the spawn:/n{}",
        go
    );
}

/// `select` receive bindings (`n, ok := <-ch:`) — same class as the loop binder.
#[test]
fn slot_loan_flushes_before_move_when_select_recv_shadows_the_loan_name() {
    let go = emit(
        "struct service {\n    replicas int\n}\n\nfn worker(m map[string]service, done chan bool) {\n    done <- true\n}\n\nfn main() {\n    m := map[string]service{\"a\": {replicas: 1}}\n    n := &m[\"a\"]\n    n.replicas = 5\n    done := make(chan bool, 1)\n    task worker(move m, done)\n    ch := make(chan int, 1)\n    select {\n        n, ok := <-ch: {\n            fmt.println(n)\n        }\n        default: {\n        }\n    }\n    <-done\n}\n",
    );
    let flush_at = go.find("m[\"a\"] = n").expect("store-back emitted");
    let spawn_at = go.find("go worker(m, done)").expect("task spawn emitted");
    assert!(
        flush_at < spawn_at,
        "a select receive binding that shadows the loan name must not defer the store-back past the spawn:/n{}",
        go
    );
}

/// The other direction: when the OUTER name really is read again after the
/// shadowing loop, the loan is still live and the store-back must NOT be
/// flushed early (that would drop the later mutation — a lost update).
#[test]
fn slot_loan_stays_live_when_the_outer_name_is_reused_after_a_shadowing_loop() {
    let go = emit(
        "struct service {\n    replicas int\n}\n\nfn main() {\n    m := map[string]service{\"a\": {replicas: 1}}\n    n := &m[\"a\"]\n    n.replicas = 5\n    for n in 0..10 {\n        fmt.println(n)\n    }\n    n.replicas = 7\n}\n",
    );
    let after_loop_use = go.find("n.replicas = 7").expect("post-loop use emitted");
    let flush_at = go.find("m[\"a\"] = n").expect("store-back emitted");
    assert!(
        after_loop_use < flush_at,
        "a post-loop read of the outer name keeps the loan live; the store-back must follow it:/n{}",
        go
    );
    assert_eq!(
        go.matches("m[\"a\"] = n").count(),
        1,
        "one store-back, at the real last use:/n{}",
        go
    );
}

/// An `if` INIT that declares the loan's name shadows it for `cond`, both arms
/// and `else` (Go's if-statement scope) — the same class as the loop binder.
/// Found by review after the first cut of this fix: the store-back still landed
/// after the spawn, because `cond`'s `n` was counted as a use of the loan.
#[test]
fn slot_loan_flushes_before_move_when_if_init_shadows_the_loan_name() {
    let go = emit(
        "struct item {\n    x int\n}\n\nfn worker(m map[string]item, done chan bool) {\n    m[\"w\"] = item{x: 1}\n    done <- true\n}\n\nfn main() {\n    m := map[string]item{\"a\": item{x: 0}}\n    done := make(chan bool, 1)\n    n := &m[\"a\"]\n    n.x = 1\n    task worker(move m, done)\n    total := 0\n    if n := 2; n > 1 {\n        total += n\n    }\n    <-done\n    fmt.println(total)\n}\n",
    );
    let flush_at = go.find("m[\"a\"] = n").expect("store-back emitted");
    let spawn_at = go.find("go worker(m, done)").expect("task spawn emitted");
    assert!(
        flush_at < spawn_at,
        "an `if` init that shadows the loan name must not defer the store-back past the spawn:/n{}",
        go
    );
}

/// A closure's NAMED RESULT is a binder too (`fn() (n int) { n = 5 }`): its
/// body assigns the result, not an outer loan of the same name (#547, same
/// class as the loop / if-init binders; found in review after the first cut).
#[test]
fn slot_loan_flushes_before_move_when_closure_named_result_shadows_the_loan_name() {
    let go = emit(
        "struct item {\n    x int\n}\n\nfn worker(m map[string]item, done chan bool) {\n    m[\"w\"] = item{x: 1}\n    done <- true\n}\n\nfn main() {\n    m := map[string]item{\"a\": item{x: 0}}\n    done := make(chan bool, 1)\n    n := &m[\"a\"]\n    n.x = 1\n    task worker(move m, done)\n    f := fn() (n int) {\n        n = 5\n        return\n    }\n    fmt.println(f())\n    <-done\n}\n",
    );
    let flush_at = go.find("m[\"a\"] = n").expect("store-back emitted");
    let spawn_at = go.find("go worker(m, done)").expect("task spawn emitted");
    assert!(
        flush_at < spawn_at,
        "a closure named result that shadows the loan name must not defer the store-back past the spawn:/n{}",
        go
    );
}

/// `match some(n)` binds `n` for the whole arm (guard included) — the third
/// binder shape, already handled upstream; this pins it so a refactor cannot
/// silently drop the `shadowed` filter (#547 review).
#[test]
fn slot_loan_flushes_before_move_when_match_binding_shadows_the_loan_name() {
    let go = emit(
        "struct service {\n    replicas int\n}\n\nfn worker(m map[string]service, done chan bool) {\n    done <- true\n}\n\nfn maybe() option[int] {\n    return some(1)\n}\n\nfn main() {\n    m := map[string]service{\"a\": {replicas: 1}}\n    n := &m[\"a\"]\n    n.replicas = 5\n    done := make(chan bool, 1)\n    task worker(move m, done)\n    match maybe() {\n        some(n): {\n            fmt.println(n)\n        }\n        none: {\n        }\n    }\n    <-done\n}\n",
    );
    let flush_at = go.find("m[\"a\"] = n").expect("store-back emitted");
    let spawn_at = go.find("go worker(m, done)").expect("task spawn emitted");
    assert!(
        flush_at < spawn_at,
        "a `match some(n)` binding that shadows the loan name must not defer the store-back past the spawn:/n{}",
        go
    );
}

/// A closure PARAM shadowing the loan name (the `results` half is covered by
/// `..._closure_named_result_...`; this pins the `params` half).
#[test]
fn slot_loan_flushes_before_move_when_closure_param_shadows_the_loan_name() {
    let go = emit(
        "struct service {\n    replicas int\n}\n\nfn worker(m map[string]service, done chan bool) {\n    done <- true\n}\n\nfn main() {\n    m := map[string]service{\"a\": {replicas: 1}}\n    n := &m[\"a\"]\n    n.replicas = 5\n    done := make(chan bool, 1)\n    task worker(move m, done)\n    f := fn(n int) {\n        fmt.println(n)\n    }\n    f(1)\n    <-done\n}\n",
    );
    let flush_at = go.find("m[\"a\"] = n").expect("store-back emitted");
    let spawn_at = go.find("go worker(m, done)").expect("task spawn emitted");
    assert!(
        flush_at < spawn_at,
        "a closure param that shadows the loan name must not defer the store-back past the spawn:/n{}",
        go
    );
}

/// The OVER-SKIP guard for the `if` arm: when the init does NOT declare the
/// loan's name, the body's use of it is a real use and the store-back must
/// stay after it (dropping this would lose the second mutation).
#[test]
fn slot_loan_stays_live_when_an_if_init_binds_a_different_name() {
    let go = emit(
        "struct service {\n    replicas int\n}\n\nfn main() {\n    m := map[string]service{\"a\": {replicas: 1}}\n    n := &m[\"a\"]\n    n.replicas = 5\n    if k := 2; k > 1 {\n        n.replicas = 7\n    }\n}\n",
    );
    let body_use = go.find("n.replicas = 7").expect("body use emitted");
    let flush_at = go.find("m[\"a\"] = n").expect("store-back emitted");
    assert!(
        body_use < flush_at,
        "an `if` whose init binds a DIFFERENT name must not shadow the loan; the store-back follows the body use:/n{}",
        go
    );
    assert_eq!(
        go.matches("m[\"a\"] = n").count(),
        1,
        "one store-back, at the real last use:/n{}",
        go
    );
}

#[test]
fn parenthesized_slot_loan_lowers_to_copy_and_storeback() {
    let go = emit(
        "struct svc {\n    count int\n}\n\nfn main() {\n    m := map[string]svc{\"a\": svc{count: 1}}\n    s := (&m[\"a\"])\n    s.count = 2\n    s = &(m[\"a\"])\n    s.count = 3\n}\n",
    );
    assert!(
        go.contains("s := m[\"a\"]"),
        "expected parenthesized slot loan to lower to copy read, got:\n{}",
        go
    );
    assert!(
        go.contains("m[\"a\"] = s"),
        "expected parenthesized slot loan to emit store-back, got:\n{}",
        go
    );
    assert!(
        !go.contains("&m[\"a\"]") && !go.contains("&(m[\"a\"])"),
        "expected no raw address-of map index in emitted Go, got:\n{}",
        go
    );
}

#[test]
fn array_composite_literals_emit_fixed_and_ellipsis_go_arrays() {
    let src = r#"
const n = 2

struct pair {
    a int
    b int
}

struct holder {
    tags [3]string
}

fn take(xs [3]string) [3]string {
    return xs
}

fn main() {
    a := [3]int{1, 2, 3}
    b := [...]int{1, 2, 3}
    c := [3]int{}
    d := [3]int{1}
    e := [n]int{1, 2}
    f := [n + 1]int{1, 2, 3}
    g := [2][2]int{{1, 2}, {3, 4}}
    h := [2]pair{{a: 1, b: 2}, {a: 3, b: 4}}
    s := take([3]string{"a", "b", "c"})
    hd := holder{tags: [3]string{"x", "y", "z"}}
    m := map[string][2]int{"k": [2]int{7, 8}}
    for i, v in [3]int{10, 20, 30} {
        fmt.println(i, v)
    }
    fmt.println(a[0], b[1], c[2], d[0], e[1], f[2], g[1][0], h[0].b, s[0], hd.tags[1], m["k"][0])
}
"#;
    let go = emit(src);
    assert!(go.contains("a := [3]int{1, 2, 3}"), ":\n{go}");
    assert!(go.contains("b := [...]int{1, 2, 3}"), ":\n{go}");
    assert!(go.contains("c := [3]int{}"), ":\n{go}");
    assert!(go.contains("d := [3]int{1}"), ":\n{go}");
    assert!(go.contains("e := [n]int{1, 2}"), ":\n{go}");
    assert!(go.contains("f := [n + 1]int{1, 2, 3}"), ":\n{go}");
    assert!(
        go.contains("g := [2][2]int{[2]int{1, 2}, [2]int{3, 4}}"),
        ":\n{go}"
    );
    assert!(
        go.contains("h := [2]pair{pair{a: 1, b: 2}, pair{a: 3, b: 4}}"),
        ":\n{go}"
    );
    assert!(
        go.contains("s := take([3]string{\"a\", \"b\", \"c\"})"),
        ":\n{go}"
    );
    assert!(
        go.contains("for i, v := range [3]int{10, 20, 30} {"),
        ":\n{go}"
    );
    if std::process::Command::new("go")
        .arg("version")
        .output()
        .is_ok_and(|o| o.status.success())
    {
        let tmp = std::env::temp_dir().join(format!("hyb_emit_arr_643_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        std::fs::create_dir_all(&tmp).expect("create temp dir");
        std::fs::write(tmp.join("go.mod"), "module arrlit643\n\ngo 1.21\n").expect("write go.mod");
        std::fs::write(tmp.join("main.go"), &go).expect("write main.go");
        let out = std::process::Command::new("go")
            .arg("run")
            .arg(".")
            .current_dir(&tmp)
            .output()
            .expect("invoke go run");
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(
            out.status.success(),
            "go run failed on emitted Go:\n{}\nstderr:\n{}",
            go,
            String::from_utf8_lossy(&out.stderr)
        );
        let stdout = String::from_utf8_lossy(&out.stdout);
        assert!(
            stdout.contains("1 2 0 1 2 3 3 2 a y 7"),
            "unexpected runtime stdout:\n{stdout}"
        );
    }
}
#[test]
fn test_emit_t9_set_surface() {
    let go = emit(
        "fn main() {\n    seen := set[int]{}\n    seen.insert(1)\n    fmt.println(len(seen))\n    delete(seen, 1)\n    for v in seen {\n        fmt.println(v)\n    }\n}\n",
    );
    assert!(go.contains("seen.len()"), "len(seen) should lower to seen.len():\n{}", go);
    assert!(go.contains("seen.remove(1)"), "delete(seen, 1) should lower to seen.remove(1):\n{}", go);
    assert!(go.contains("for v := range seen.m {"), "for v in seen should lower to for v := range seen.m:\n{}", go);
    assert!(go.contains("type set[t comparable] struct {"), "SET_PRELUDE should be included:\n{}", go);
}

#[test]
fn test_emit_t20_option_surface() {
    let go = emit(
        "fn main() {\n    o := some(5)\n    fmt.println(o.unwrap())\n    fmt.println(o.or(0))\n    fmt.println(o.is_some())\n}\n",
    );
    assert!(go.contains("func (o option[v]) unwrap() v {"), "unwrap method in prelude:\n{}", go);
    assert!(go.contains("func (o option[v]) or(defaultVal v) v {"), "or method in prelude:\n{}", go);
    assert!(go.contains("func (o option[v]) is_some() bool {"), "is_some method in prelude:\n{}", go);
}

#[test]
fn break_loop_and_continue_loop_in_nested_for_bind_nearest_loop_issue_654() {
    let src = r#"
package main

import "fmt"

fn main() {
    rounds := 0
    loop {
        rounds += 1
        if rounds == 4 {
            fmt.println("loop guard hit")
            break loop
        }
        for i in 0..3 {
            if i == 1 {
                break loop
            }
        }
        fmt.println("after for")
    }

    visits := 0
    passes := 0
    loop {
        passes += 1
        if passes == 3 {
            break loop
        }
        for v in []int{1, 2, 3} {
            visits += v
            continue loop
        }
        fmt.println("after for (continue)")
    }
}
"#;
    let go = emit(src);
    // Outer loop 1 receives hyLoop:
    assert!(go.contains("hyLoop:\n\tfor {"), "outer loop 1 must have hyLoop label:\n{}", go);
    // Inner for i in 0..3 must NOT receive a label
    assert!(!go.contains("hyLoop1:\n\t\tfor i :="), "inner for must not receive a label:\n{}", go);
    // break loop inside inner for must break hyLoop
    assert!(go.contains("break hyLoop\n"), "break loop in inner for must target hyLoop:\n{}", go);

    // Outer loop 2 receives hyLoop1:
    assert!(go.contains("hyLoop1:\n\tfor {"), "outer loop 2 must have hyLoop1 label:\n{}", go);
    // continue loop inside inner for must continue hyLoop1
    assert!(go.contains("continue hyLoop1\n"), "continue loop in inner for must target hyLoop1:\n{}", go);
}

#[test]
fn break_loop_inside_for_without_loop_statement_refused_emit() {
    let src = r#"
fn main() {
    for i in 0..3 {
        if i == 1 {
            break loop
        }
    }
}
"#;
    let err = emit_err(src);
    assert!(
        err.contains("break loop` with no enclosing loop"),
        "expected unsupported error for break loop outside loop statement, got: {}",
        err
    );
}

#[test]
fn errors_wrap_lowers_to_fmt_errorf_and_preserves_errors_is() {
    let src = r#"
package main

import "errors"
import "fmt"

fn main() {
    not_found := errors.new("not found")
    err := errors.wrap(not_found, "opening config")
    fmt.println(err)
    fmt.println(errors.is(err, not_found))
}
"#;
    let go = emit(src);
    assert!(
        go.contains("err := fmt.Errorf(\"%s: %w\", \"opening config\", not_found)"),
        "expected errors.wrap to lower to fmt.Errorf with msg and err, got:\n{}",
        go
    );
    assert!(
        go.contains("\"errors\""),
        "must import errors when errors.New or errors.Is is used, got:\n{}",
        go
    );
    assert!(
        go.contains("\"fmt\""),
        "must import fmt for fmt.Errorf and fmt.Println, got:\n{}",
        go
    );
}

#[test]
fn errors_wrap_alone_omits_unused_errors_import() {
    let src = r#"
package main

import "errors"

fn wrap_it(err error) error {
    return errors.wrap(err, "context")
}
"#;
    let go = emit(src);
    assert!(
        go.contains("return fmt.Errorf(\"%s: %w\", \"context\", err)"),
        "expected errors.wrap to lower to fmt.Errorf, got:\n{}",
        go
    );
    assert!(
        go.contains("\"fmt\""),
        "must auto-import fmt for fmt.Errorf, got:\n{}",
        go
    );
    assert!(
        !go.contains("\"errors\""),
        "must prune unused errors import when only errors.wrap is used, got:\n{}",
        go
    );
}

#[test]
fn errors_wrap_named_arguments_and_arg_count_validation() {
    let src = r#"
package main

import "errors"

fn wrap_named(err error) error {
    return errors.wrap(msg="ctx", err=err)
}
"#;
    let go = emit(src);
    assert!(
        go.contains("return fmt.Errorf(\"%s: %w\", \"ctx\", err)"),
        "expected named args to order correctly in fmt.Errorf, got:\n{}",
        go
    );

    let err_src = r#"
package main

import "errors"

fn f(err error) error {
    return errors.wrap(err)
}
"#;
    let err = emit_err(err_src);
    assert!(
        err.contains("errors.wrap takes exactly two arguments"),
        "expected argument count error, got: {}",
        err
    );
}

#[test]
fn m4_initialism_mapping_for_foreign_go_names_issue_658() {
    let go = emit(
        "fn main() {\n    _ = io.eof\n    _ = http.status_ok\n    _ = time.rfc3339\n    _ = strings.to_valid_utf8(\"a\", \"b\")\n}\n",
    );
    assert!(go.contains("io.EOF"), "expected io.EOF, got:\n{}", go);
    assert!(go.contains("http.StatusOK"), "expected http.StatusOK, got:\n{}", go);
    assert!(go.contains("time.RFC3339"), "expected time.RFC3339, got:\n{}", go);
    assert!(go.contains("strings.ToValidUTF8(\"a\", \"b\")"), "expected strings.ToValidUTF8, got:\n{}", go);
}


#[test]
fn package_mode_export_normalization_collision_issue_1421() {
    let check_collision = |src: &str, expected_msg_part: &str| {
        let file = parse_file("t.hy", src).expect("parse");
        match super::emit_go_package(&file, "demo", "demo") {
            Ok(go) => panic!("expected collision error, got:\n{}", go),
            Err(e) => {
                assert!(
                    e.message.contains(expected_msg_part),
                    "expected '{}' in error, got: {}",
                    expected_msg_part,
                    e.message
                );
            }
        }
    };

    // 1. Initialism splitting collision: read_url vs read_u_r_l
    let src1 = "\
pub fn read_url() int {
    return 1
}
pub fn read_u_r_l() int {
    return 2
}
";
    check_collision(src1, "exported function name collision: `read_url` and `read_u_r_l` both normalize to `ReadURL` in package mode");

    // 2. Declaration order independent: read_u_r_l then read_url
    let src2 = "\
pub fn read_u_r_l() int {
    return 2
}
pub fn read_url() int {
    return 1
}
";
    check_collision(src2, "exported function name collision: `read_u_r_l` and `read_url` both normalize to `ReadURL` in package mode");

    // 3. Repeated underscore collision: read_url vs read__url
    let src3 = "\
pub fn read_url() int {
    return 1
}
pub fn read__url() int {
    return 3
}
";
    check_collision(src3, "exported function name collision: `read_url` and `read__url` both normalize to `ReadURL` in package mode");

    // 4. Control: single public function succeeds in package mode
    let src4 = "\
pub fn read_url() int {
    return 1
}
";
    let file4 = parse_file("t.hy", src4).expect("parse");
    let go4 = super::emit_go_package(&file4, "demo", "demo").expect("emit package");
    assert!(go4.contains("func ReadURL() int {"));

    // 5. Control: private functions with colliding normalized names do not export in package mode
    let src5 = "\
fn read_url() int {
    return 1
}
fn read_u_r_l() int {
    return 2
}
";
    let file5 = parse_file("t.hy", src5).expect("parse");
    let go5 = super::emit_go_package(&file5, "demo", "demo").expect("emit package");
    assert!(go5.contains("func read_url() int {"));
    assert!(go5.contains("func read_u_r_l() int {"));

    // 6. Control: ordinary binary mode (emit_go) is unaffected
    let src6 = "\
pub fn read_url() int {
    return 1
}
pub fn read_u_r_l() int {
    return 2
}
fn main() {}
";
    let go6 = emit(src6);
    assert!(go6.contains("func read_url() int {"));
    assert!(go6.contains("func read_u_r_l() int {"));
}

#[test]
fn utf8_byte_escapes_emit_exact_utf8_strings_issue_1199() {
    let src = "fn main() {\n    value := 7\n    s1 := \"\\xc3\\xa9\"\n    s2 := \"\\303\\251\"\n    s3 := \"\\xe4\\xb8\\xad\"\n    s4 := \"\\xf0\\x9f\\x99\\x82\"\n    s5 := \"prefix:\\xc3\\xa9:{value}\"\n    c1 := '\\xe9'\n    c2 := '\\u{e9}'\n    fmt.println(len(s1), s1 == \"é\", s2 == \"é\", s3 == \"中\", s4 == \"🙂\", s5 == \"prefix:é:7\", c1 == c2)\n}\n";
    let go = emit(src);
    assert!(go.contains("s1 := \"é\""), "expected s1 := \"é\", got:\n{}", go);
    assert!(go.contains("s2 := \"é\""), "expected s2 := \"é\", got:\n{}", go);
    assert!(go.contains("s3 := \"中\""), "expected s3 := \"中\", got:\n{}", go);
    assert!(go.contains("s4 := \"🙂\""), "expected s4 := \"🙂\", got:\n{}", go);
    assert!(
        go.contains("fmt.Sprintf(\"prefix:é:%v\", value)"),
        "expected interpolated prefix:é:%v, got:\n{}",
        go
    );
}
