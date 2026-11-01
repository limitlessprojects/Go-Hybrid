// Regression tests for #2315: the deferred closure check must see the
// environment as it was AT THE CLOSURE LITERAL, not at block exit.
//
// The bug: a declaration placed after the literal that shadows a captured
// binding replaced the captured binding in the deferred check, so a leak the
// check should have caught went silent. The tests below pin both directions —
// the shadow must not hush the leak, and a real `move` of a binding that WAS
// visible must still propagate.

#[test]
fn shadow_after_closure_literal_does_not_hide_the_captured_map() {
    // `m` (a map, not sendable) is captured by the closure. `m := 1` later in
    // the same block shadows it. Before the fix the deferred check saw the int
    // and stayed quiet; it must still see the map.
    let src = "fn worker(m map[string]int, done chan<- bool) {\n    m[\"worker\"] = 1\n    done <- true\n}\nfn main() {\n    m := map[string]int{\"init\": 0}\n    done := make(chan bool, 1)\n    if true {\n        f := fn() {\n            task worker(m, done)\n        }\n        f()\n        m := 1\n        _ = m\n    }\n    m[\"main\"] = 2\n    <-done\n}\n";
    let r = hy_check::check_file(&hy_syntax::parse_file("repro.hy", src).unwrap());
    assert!(
        !r.diagnostics.is_empty(),
        "the captured non-sendable map must still be diagnosed after the shadow"
    );
}

#[test]
fn shadow_after_closure_literal_does_not_hide_a_moved_binding() {
    // Same shape, but `m` was already moved on the `task` line before the
    // literal. A `move` of a binding that WAS visible must still propagate
    // through the deferred check rather than being replaced by the shadow.
    let src = "fn worker(m map[string]int, done chan<- bool) {\n    m[\"worker\"] = 1\n    done <- true\n}\nfn main() {\n    m := map[string]int{\"init\": 0}\n    done := make(chan bool, 1)\n    task worker(move m, done)\n    if true {\n        f := fn() {\n            m[\"closure\"] = 2\n        }\n        f()\n        m := 1\n        _ = m\n    }\n    <-done\n}\n";
    let r = hy_check::check_file(&hy_syntax::parse_file("repro.hy", src).unwrap());
    assert!(
        !r.diagnostics.is_empty(),
        "a real move of a visible binding must still be diagnosed"
    );
}

#[test]
fn closure_without_a_later_shadow_is_unchanged() {
    // Guard: removing the shadow must not change the verdict either way. This
    // is the same program as the first test with `m := 1` deleted, so whatever
    // the checker decides here it must decide for the same reason.
    let src = "fn worker(m map[string]int, done chan<- bool) {\n    m[\"worker\"] = 1\n    done <- true\n}\nfn main() {\n    m := map[string]int{\"init\": 0}\n    done := make(chan bool, 1)\n    if true {\n        f := fn() {\n            task worker(m, done)\n        }\n        f()\n    }\n    m[\"main\"] = 2\n    <-done\n}\n";
    let r = hy_check::check_file(&hy_syntax::parse_file("repro.hy", src).unwrap());
    assert!(
        !r.diagnostics.is_empty(),
        "the baseline leak must be diagnosed with or without a shadow, got a clean verdict"
    );
}
