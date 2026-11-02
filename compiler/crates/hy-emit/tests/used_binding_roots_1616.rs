//! #1616 — `used_binding_roots` scanned the whole emitted Go source for
//! `<binding_root>.`, so a string literal such as `"jsonbinding.Parse"`
//! selected the Rust/cgo bridge even though no binding was called. Mask
//! literals and comments before the scan; real calls must still be found.

use hy_emit::used_binding_roots;

#[test]
fn binding_calls_are_detected() {
    assert_eq!(
        used_binding_roots("v := jsonbinding.Parse(b)"),
        vec!["jsonbinding"]
    );
    assert_eq!(
        used_binding_roots("_ = sha3binding.Digest256(x)"),
        vec!["sha3binding"]
    );
}

#[test]
fn string_literals_do_not_trigger_bindings() {
    assert!(used_binding_roots("s := \"jsonbinding.Parse\"").is_empty());
    assert!(used_binding_roots("s := `sha3binding.Digest256`").is_empty());
    assert!(used_binding_roots("r := 'j'").is_empty());
}

#[test]
fn comments_do_not_trigger_bindings() {
    assert!(used_binding_roots("// jsonbinding.Parse\nx := 1").is_empty());
    assert!(used_binding_roots("/* sha3binding.Digest256 */ x := 1").is_empty());
}

#[test]
fn escaped_quotes_keep_the_rest_scannable() {
    let src = "s := \"a\\\"jsonbinding.Parse\"\nx := jsonbinding.Parse(s)";
    assert_eq!(used_binding_roots(src), vec!["jsonbinding"]);
}

#[test]
fn import_path_alone_is_not_a_call() {
    assert!(used_binding_roots("import \"hybrid.program/jsonbinding\"").is_empty());
}
