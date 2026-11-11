use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn assert_rejects_e2011(src: &str, expected_substr: &str) {
    let ast = parse_file("test.hy", src).expect("must parse");
    let res = check_file(&ast);
    assert_eq!(
        res.verdict,
        Verdict::Reject,
        "expected Reject verdict, got: {:#?}",
        res.diagnostics
    );
    let e2011: Vec<_> = res
        .diagnostics
        .iter()
        .filter(|d| d.code == "HYB-E2011")
        .collect();
    assert!(
        !e2011.is_empty(),
        "expected HYB-E2011, got diagnostics: {:#?}",
        res.diagnostics
    );
    assert!(
        e2011.iter().any(|d| d.message.contains(expected_substr)),
        "expected HYB-E2011 message containing `{expected_substr}`, got: {e2011:#?}"
    );
}

fn assert_accepts(src: &str) {
    let ast = parse_file("test.hy", src).expect("must parse");
    let res = check_file(&ast);
    assert_eq!(
        res.verdict,
        Verdict::Accept,
        "expected Accept verdict, got diagnostics: {:#?}",
        res.diagnostics
    );
}

#[test]
fn test_generic_free_fn_non_error_and_option_try_rejects() {
    assert_rejects_e2011(
        r#"
fn id[t](x t) t {
    return x
}

fn run() error {
    _ = id[int](42)?
    return nil
}
"#,
        "whose results do not end in `error`",
    );

    assert_rejects_e2011(
        r#"
fn find_item[t](x t) option[t] {
    return some(x)
}

fn run() error {
    _ = find_item[string]("k")?
    return nil
}
"#,
        "`?` on `option` is a type error",
    );

    assert_rejects_e2011(
        r#"
fn id[t](x t) t {
    return x
}

fn run() error {
    _ = (id[int])(42)?
    return nil
}
"#,
        "whose results do not end in `error`",
    );
}

#[test]
fn test_associated_fn_non_error_and_option_try_rejects() {
    assert_rejects_e2011(
        r#"
struct box {
    v int
}

fn box.new(v int) box {
    return box{v: v}
}

fn run() error {
    _ = box.new(1)?
    return nil
}
"#,
        "whose results do not end in `error`",
    );

    assert_rejects_e2011(
        r#"
struct cache[k, v] {
    cap int
}

fn cache.new[k, v](cap int) cache[k, v] {
    return cache[k, v]{cap: cap}
}

fn run() error {
    _ = cache.new[string, int](8)?
    return nil
}
"#,
        "whose results do not end in `error`",
    );

    assert_rejects_e2011(
        r#"
struct box {
    v int
}

fn box.lookup(k int) option[int] {
    _ = k
    return none
}

fn run() error {
    _ = box.lookup(1)?
    return nil
}
"#,
        "`?` on `option` is a type error",
    );
}

#[test]
fn test_user_struct_method_non_error_and_option_try_rejects() {
    assert_rejects_e2011(
        r#"
struct counter {
    n int
}

fn (c *counter) value() int {
    return c.n
}

fn run(c *counter) error {
    _ = c.value()?
    return nil
}
"#,
        "whose results do not end in `error`",
    );

    assert_rejects_e2011(
        r#"
struct counter {
    n int
}

fn (c counter) peek() option[int] {
    return some(c.n)
}

fn run() error {
    c := counter{n: 3}
    _ = c.peek()?
    return nil
}
"#,
        "`?` on `option` is a type error",
    );

    assert_rejects_e2011(
        r#"
struct counter {
    n int
}

struct holder {
    inner counter
}

fn (c counter) value() int {
    return c.n
}

fn make_counter() counter {
    return counter{n: 1}
}

fn run(h holder) error {
    _ = (counter{n: 1}).value()?
    _ = h.inner.value()?
    _ = make_counter().value()?
    return nil
}
"#,
        "whose results do not end in `error`",
    );
}

#[test]
fn test_fn_typed_bindings_and_struct_fields_try_rejects() {
    assert_rejects_e2011(
        r#"
fn run(cb fn() int) error {
    _ = cb()?
    return nil
}
"#,
        "whose results do not end in `error`",
    );

    assert_rejects_e2011(
        r#"
fn run(cb fn() option[int]) error {
    _ = cb()?
    return nil
}
"#,
        "`?` on `option` is a type error",
    );

    assert_rejects_e2011(
        r#"
struct runner {
    hook fn() int
}

fn run(r runner) error {
    local_cb := fn() int { return 7 }
    _ = local_cb()?
    _ = r.hook()?
    return nil
}
"#,
        "whose results do not end in `error`",
    );
}

#[test]
fn test_error_returning_generics_assoc_methods_and_shadowing_accept() {
    assert_accepts(
        r#"
struct box[t] {
    v t
}

fn parse_item[t](x t) (t, error) {
    return x, nil
}

fn box.try_new[t](v t) (box[t], error) {
    return box[t]{v: v}, nil
}

fn (b *box[t]) load() (t, error) {
    return b.v, nil
}

fn non_err() int {
    return 0
}

fn run(cb fn() error) error {
    v := parse_item[int](10)?
    b := box.try_new[int](v)?
    _ = b.load()?
    cb()?
    non_err := fn() (int, error) { return 1, nil }
    _ = non_err()?
    return nil
}
"#,
    );
}
