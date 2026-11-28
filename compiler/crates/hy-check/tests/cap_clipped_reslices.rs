use hy_check::{check_file, Verdict};
use hy_syntax::{parse_file, Expr, Stmt};

#[test]
fn plan_uses_checker_kinds_for_slices_arrays_pointers_and_aliases() {
    for (declarations, signature, base) in [
        ("", "s []int", "s"),
        ("", "s [4]int", "s"),
        ("", "s *[4]int", "s"),
        ("type ints = []int\n", "s ints", "s"),
        ("type array = [4]int\n", "s *array", "s"),
        ("struct holder { data []int }\n", "s holder", "s.data"),
        ("struct holder { data [4]int }\n", "s *holder", "s.data"),
    ] {
        let source = format!("{declarations}fn view({signature}) []int {{ return {base}[1:2] }}");
        let file = parse_file("plan.hy", &source).unwrap();
        let result = check_file(&file);
        assert_eq!(result.verdict, Verdict::Accept, "{source}: {:?}", result.diagnostics);
        let f = file.fns().next().unwrap();
        let Stmt::Return { values, .. } = &f.body.as_ref().unwrap().stmts[0] else { panic!("return expected") };
        assert!(result.clip_plan.clips(values[0].span()), "missing plan for {source}");
    }
}

#[test]
fn nested_slice_spans_are_distinct_and_both_planned() {
    let file = parse_file("nested.hy", "fn view(s []int) []int { return s[:3][1:2] }").unwrap();
    let result = check_file(&file);
    assert_eq!(result.verdict, Verdict::Accept, "{:?}", result.diagnostics);
    let Stmt::Return { values, .. } = &file.fns().next().unwrap().body.as_ref().unwrap().stmts[0] else { panic!() };
    let Expr::Slice { base, .. } = &values[0] else { panic!() };
    assert_eq!(values[0].span().start, base.span().start);
    assert_ne!(values[0].span().end, base.span().end);
    assert!(result.clip_plan.clips(values[0].span()));
    assert!(result.clip_plan.clips(base.span()));
}

#[test]
fn explicit_full_slices_do_not_request_clipping() {
    let file = parse_file("full.hy", "fn view(s []int) []int { return s[1:2:4] }").unwrap();
    let result = check_file(&file);
    assert_eq!(result.verdict, Verdict::Accept, "{:?}", result.diagnostics);
    let Stmt::Return { values, .. } = &file.fns().next().unwrap().body.as_ref().unwrap().stmts[0] else { panic!() };
    assert!(!result.clip_plan.clips(values[0].span()));
}

fn rejects(source: &str, code: &str) {
    let file = parse_file("negative.hy", source).expect("negative case must parse");
    let result = check_file(&file);
    assert_eq!(result.verdict, Verdict::Reject, "accepted {source}");
    assert!(result.diagnostics.iter().any(|d| d.code == code), "{source}: {:?}", result.diagnostics);
}

#[test]
fn full_slice_requires_integer_nonnegative_ordered_bounds_within_array_capacity() {
    for expr in ["s[:1:true]", "s[:1:1.5]", "s[:1:\"bad\"]", "s[:1:-1]",
                 "s[:3:2]", "s[3:2:4]", "s[3:hi:2]"] {
        rejects(&format!("fn f(s []int, hi int) {{ _ = {expr} }}"), "HYB-E1002");
    }
    for base in ["a", "p"] {
        rejects(&format!("fn f() {{
var a [4]int
p := &a
_ = p
_ = {base}[:1:5]
}}"), "HYB-E1002");
    }
    rejects("fn f(s string) { _ = s[:1:2] }", "HYB-E1002");
}

#[test]
fn maximum_is_walked_for_names_arity_builtins_conversions_and_arithmetic() {
    for (declarations, expr, code) in [
        ("", "missing", "HYB-E1001"),
        ("fn value() int { return 4 }", "value(1)", "HYB-E1002"),
        ("fn pair() (int, int) { return 3, 4 }", "pair()", "HYB-E2002"),
        ("fn no_value() {}", "no_value()", "HYB-E1002"),
        ("", "cap(1)", "HYB-E2005"),
        ("", "1 / 0", "HYB-E6001"),
        ("", "int(true)", "HYB-E1002"),
        ("", "<-1", "HYB-E2008"),
    ] {
        rejects(&format!("{declarations}\nfn f(s []int) {{ _ = s[:1:{expr}] }}"), code);
    }
}

#[test]
fn narrowing_in_all_bounds_matches_existing_truncating_conversions() {
    // SPEC X4 and const_overflow.rs deliberately fold int8(128) to -128.
    // Existing lo/hi reject the negative index with E1002, not E6002.
    for expr in ["s[int8(128):]", "s[:int8(128)]", "s[:1:int8(128)]"] {
        rejects(&format!("fn f(s []int) {{ _ = {expr} }}"), "HYB-E1002");
    }
    for expr in ["s[int8(256):]", "s[:int8(257)]", "s[:1:int8(258)]"] {
        let file = parse_file("truncation.hy", &format!("fn f(s []int) {{ _ = {expr} }}")).unwrap();
        let result = check_file(&file);
        assert_eq!(result.verdict, Verdict::Accept, "{expr}: {:?}", result.diagnostics);
    }
}

#[test]
fn helper_names_are_stable_across_path_spelling_and_distinct_for_siblings() {
    let source = "fn view(s []int) []int { return s[:1] }";
    let check = |path| check_file(&parse_file(path, source).unwrap()).clip_plan;
    let relative = check("one.hy");
    let absolute = check("/some/project/one.hy");
    let sibling = check("two.hy");
    assert_eq!(relative.helper_name(), absolute.helper_name());
    assert_ne!(relative.helper_name(), sibling.helper_name());

    let shadowed_source = format!(
        "fn view({} int, s []int) []int {{ return s[:1] }}",
        relative.helper_name(),
    );
    let shadowed = check_file(&parse_file("one.hy", &shadowed_source).unwrap());
    assert_eq!(shadowed.verdict, Verdict::Accept, "{:?}", shadowed.diagnostics);
    assert_ne!(shadowed.clip_plan.helper_name(), relative.helper_name());
    assert_ne!(shadowed.clip_plan.helper_name(), sibling.helper_name());
}

#[test]
fn maximum_use_preserves_move_and_borrow_checks() {
    rejects(r#"
fn consume(s []int) int { return len(s) }
fn worker(s []int, done chan bool) { done <- true }
fn main() {
    s := []int{1, 2, 3, 4}
    done := make(chan bool, 1)
    task worker(move s, done)
    other := []int{1, 2, 3, 4}
    _ = other[:1:consume(s)]
    <-done
}
"#, "HYB-E3003");
}

#[test]
fn maximum_only_locals_and_full_array_controls_are_accepted() {
    let file = parse_file("control.hy", r#"
fn f() {
    var a [4]int
    p := &a
    maximum := 4
    _ = a[0:1:maximum]
    _ = p[:1:maximum]
    s := make([]int, 1, 8)
    _ = s[:4:8]
}
"#).unwrap();
    let result = check_file(&file);
    assert_eq!(result.verdict, Verdict::Accept, "{:?}", result.diagnostics);
}
