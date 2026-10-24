use hy_emit::{emit_go, emit_go_package, used_binding_roots};
use hy_syntax::parse_file;

#[test]
fn main_wrapper_deduplicates_explicit_path_filepath_import() {
    let src = r#"import "fmt"
import "path/filepath"

fn helper() (string, error) {
    return filepath.clean("a//b"), nil
}

fn main() {
    cleaned := helper()?
    fmt.println(cleaned)
}
"#;
    let file = parse_file("test.hy", src).expect("parse succeeds");
    let go = emit_go(&file).expect("emit succeeds");
    let count = go.matches("\"path/filepath\"").count();
    assert_eq!(
        count, 1,
        "expected exactly one `\"path/filepath\"` import when `main` wraps `?` alongside explicit import, got {count} in:\n{go}"
    );
}

#[test]
fn used_binding_roots_ignores_string_literals_and_detects_real_imports() {
    let literal_only_src = r#"import "fmt"

fn main() {
    msg := "sha3binding.Digest256 and jsonbinding.Parse failed"
    fmt.println(msg)
}
"#;
    let literal_file = parse_file("lit.hy", literal_only_src).expect("parse succeeds");
    let literal_go = emit_go(&literal_file).expect("emit succeeds");
    assert!(
        used_binding_roots(&literal_go).is_empty(),
        "string literals mentioning `<binding_root>.` must not mark Rust bindings as used:\n{literal_go}"
    );

    let real_src = r#"import "rust/sha3"

pub fn hash_word(w string) (string, error) {
    return sha3.digest256(w)
}
"#;
    let real_file = parse_file("real.hy", real_src).expect("parse succeeds");
    let real_go =
        emit_go_package(&real_file, "demo", "example.com/demo/internal").expect("emit succeeds");
    assert_eq!(
        used_binding_roots(&real_go),
        vec!["sha3binding"],
        "real imported Rust binding root must be detected across custom binding prefixes:\n{real_go}"
    );
}
