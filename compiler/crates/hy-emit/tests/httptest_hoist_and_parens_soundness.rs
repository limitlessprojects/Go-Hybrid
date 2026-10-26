use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit_src(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parses");
    emit_go(&ast).expect("emits")
}

#[test]
fn nested_try_in_httptest_args_hoists_properly() {
    let go = emit_src(
        "\
fn make_path() (string, error) {
    return \"/ok\", nil
}

fn main() {
    mux := http.new_serve_mux()
    code, body := httptest.get(mux, make_path()?)?
    fmt.println(code, body)
}
",
    );
    assert!(go.contains("hyT0, hyErr1 := make_path()"), "got:\n{go}");
    assert!(go.contains("http.NewRequest(\"GET\", hyT0, nil)"), "got:\n{go}");
    assert!(go.contains("httptest.NewRecorder()"), "got:\n{go}");
    assert!(go.contains("mux.ServeHTTP("), "got:\n{go}");
    assert!(go.contains("code, body := "), "got:\n{go}");
}

#[test]
fn parenthesized_httptest_forms_and_parens_defines_lower_properly() {
    let go = emit_src(
        "\
fn make_path() (string, error) {
    return \"/ok\", nil
}

fn main() {
    mux := http.new_serve_mux()
    c1, b1 := (httptest.get(mux, \"/ok\")?)
    c2, b2 := (httptest.get(mux, \"/ok\"))?
    c3, b3 := (httptest).get(mux, \"/ok\")?
    c4, b4 := ((httptest).post(mux, make_path()?)?)
    fmt.println(c1, b1, c2, b2, c3, b3, c4, b4)
}
",
    );
    assert!(go.contains("http.NewRequest(\"GET\", \"/ok\", nil)"), "got:\n{go}");
    assert!(go.contains("http.NewRequest(\"POST\", hyT9, nil)"), "got:\n{go}");
    assert!(go.contains("c1, b1 :="), "got:\n{go}");
    assert!(go.contains("c2, b2 :="), "got:\n{go}");
    assert!(go.contains("c3, b3 :="), "got:\n{go}");
    assert!(go.contains("c4, b4 :="), "got:\n{go}");
}

#[test]
fn parenthesized_try_define_lowers_properly() {
    let go = emit_src(
        "\
fn compute() (int, error) {
    return 42, nil
}

fn main() {
    a := (compute()?)
    b := (compute())?
    fmt.println(a, b)
}
",
    );
    assert!(go.contains("a, hyErr0 := compute()"), "got:\n{go}");
    assert!(go.contains("b, hyErr1 := compute()"), "got:\n{go}");
}

#[test]
fn shadowed_httptest_local_var_not_hijacked() {
    let go = emit_src(
        "\
struct client {}

fn (c *client) get(path string) (int, error) {
    return 200, nil
}

fn main() {
    httptest := &client{}
    code := httptest.get(\"/ping\")?
    fmt.println(code)
}
",
    );
    assert!(go.contains("code, hyErr0 := httptest.get(\"/ping\")"), "got:\n{go}");
    assert!(!go.contains("http.NewRequest"), "got:\n{go}");
}
