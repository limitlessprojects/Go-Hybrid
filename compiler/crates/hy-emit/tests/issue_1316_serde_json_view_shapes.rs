//! Regression tests for #1316: `serde_json.view` lowers to `jsonbinding.Value`
//! via `RUST_TYPES` and exposes the full `serde_json` handle method surface in
//! `SHAPES` and `METHOD_RESULTS`.

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parse");
    emit_go(&ast).expect("emit_go")
}

#[test]
fn serde_json_view_param_and_return_type_lower_to_jsonbinding_value() {
    let go = emit(
        r#"import "rust/serde_json"

fn read_view(v serde_json.view) (string, error) {
    name := v.string("name")?
    v.set("seen", "yes")?
    v.remove("temp")?
    return name, nil
}
"#,
    );
    assert!(
        go.contains("v jsonbinding.Value"),
        "expected serde_json.view parameter to lower to jsonbinding.Value, got:\n{go}"
    );
    assert!(
        !go.contains("serde_json.View"),
        "must not emit nonexistent serde_json.View type, got:\n{go}"
    );
    assert!(
        go.contains("v.String(\"name\")"),
        "expected v.string(\"name\") to resolve to v.String(\"name\"), got:\n{go}"
    );
    assert!(
        go.contains("hyErr1 := v.Set(\"seen\", \"yes\")"),
        "expected v.set(...) on serde_json.view to hoist as 0-value error check, got:\n{go}"
    );
    assert!(
        go.contains("hyErr2 := v.Remove(\"temp\")"),
        "expected v.remove(...) on serde_json.view to hoist as 0-value error check, got:\n{go}"
    );
}

#[test]
fn serde_json_get_and_array_yield_view_bindings_with_method_shapes() {
    let go = emit(
        r#"import "rust/serde_json"

fn inspect(doc serde_json.value) (int64, error) {
    sub := doc.get("profile")?
    age := sub.int("age")?
    return age, nil
}
"#,
    );
    assert!(
        go.contains("doc.Get(\"profile\")"),
        "expected doc.get to lower to doc.Get, got:\n{go}"
    );
    assert!(
        go.contains("sub.Int(\"age\")"),
        "expected sub (inferred as serde_json.view) to lower sub.int to sub.Int, got:\n{go}"
    );
}
