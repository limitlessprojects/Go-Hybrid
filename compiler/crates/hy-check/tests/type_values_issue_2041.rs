//! Type identifiers are legal in type/qualifier positions, but are not values.
use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    check_file(&parse_file("type_value.hy", src).expect("regression must parse"))
}

fn rejects_type(src: &str, name: &str) {
    let result = check(src);
    assert_eq!(result.verdict, Verdict::Reject, "{src}\n{result:?}");
    assert!(
        result.diagnostics.iter().any(|d| d.code == "HYB-E1002"
            && d.message.contains(name)
            && d.message.contains("is a type, not a value")),
        "expected type-as-value diagnostic for {name}: {result:?}"
    );
}

fn accepts(src: &str) {
    let result = check(src);
    assert!(result.diagnostics.is_empty(), "{src}\n{result:?}");
    assert_eq!(result.verdict, Verdict::Accept);
}

#[test]
fn builtin_types_are_not_assignment_values() {
    for name in ["any", "error"] {
        rejects_type(&format!("fn main() {{ x := {name}\n_ = x }}"), name);
    }
}

#[test]
fn builtin_types_are_not_arguments_or_returns() {
    rejects_type("fn take(x any) {}\nfn main() { take(any) }", "any");
    rejects_type("fn bad() any { return error }", "error");
}

#[test]
fn empty_and_generic_struct_names_are_not_values() {
    for (decl, name) in [
        ("struct empty {}", "empty"),
        ("struct box[t any] {}", "box"),
        ("struct box[t any] { value t }", "box[int]"),
    ] {
        rejects_type(
            &format!("{decl}\nfn main() {{ _ = {name} }}"),
            name.split('[').next().unwrap(),
        );
    }
}

#[test]
fn qualified_types_are_not_values() {
    for (path, root, member) in [
        ("net/http", "http", "client"),
        ("sync", "sync", "mutex"),
        ("time", "time", "duration"),
        ("encoding/json", "json", "decoder"),
        ("io", "io", "reader"),
        ("sync/atomic", "atomic", "int64"),
    ] {
        let name = format!("{root}.{member}");
        rejects_type(
            &format!("import \"{path}\"\nfn main() {{ _ = {name} }}"),
            &name,
        );
    }
}

#[test]
fn qualified_types_are_not_arguments_or_returns() {
    rejects_type(
        "import \"net/http\"\nfn take(x any) {}\nfn main() { take((http.client)) }",
        "http.client",
    );
    rejects_type(
        "import \"encoding/json\"\nfn bad() any { return json.decoder }",
        "json.decoder",
    );
}

#[test]
fn builtin_values_can_shadow_type_names() {
    accepts("fn add(any int, error int) int { return any + error }\nfn main() { _ = add(1, 2) }");
    accepts("struct box[t any] {}\nfn main() { box := 3\n_ = box }");
    accepts("const any = 3\nfn main() { _ = any }");
    accepts("fn error() int { return 3 }\nfn main() { f := error\n_ = f() }");
}

#[test]
fn qualified_conversions_and_type_arguments_are_legal() {
    accepts("import \"time\"\nfn main() { _ = (time.duration)(3)\n_ = time.duration(4) }");
    accepts("fn main() { _ = any(3)\n_ = error(nil) }");
    accepts("import \"net/http\"\nfn main() { _ = new(http.client) }");
    accepts("import \"net/http\"\nfn main() { _ = http.client{} }");
    accepts("struct box[t any] {}\nfn main() { _ = box[int]{}\n_ = new(box[int]) }");
}

#[test]
fn package_values_and_type_qualified_methods_remain_legal() {
    accepts("import \"net/http\"\nfn main() { f := http.get\n_ = f\n_ = http.default_client }");
    accepts("import \"time\"\nfn main() { _ = time.second\n_ = time.duration.string }");
}

#[test]
fn local_and_parameter_fields_do_not_resolve_as_package_types() {
    accepts("import \"net/http\"\nstruct holder { client int }\nfn add(http holder) int { return http.client }\nfn main() { _ = http.default_client\nhttp := holder{client: 3}\n_ = http.client }");
}
