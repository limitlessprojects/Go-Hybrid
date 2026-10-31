use hy_check::check_file;
use hy_syntax::parse_file;

fn check_codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("test source must parse");
    check_file(&ast)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn a3_method_receiver_mutation_via_helper_and_closures() {
    let src_helper = "\
struct svc {
    count int
}

struct env {
    m map[string]svc
}

fn (e *env) bump() {
    e.m[\"b\"] = svc{count: 2}
}

fn helper(e *env) {
    e.bump()
}

fn main() {
    e := env{m: map[string]svc{\"a\": svc{count: 1}}}
    s := &e.m[\"a\"]
    helper(&e)
    s.count = 99
}
";
    assert!(
        check_codes(src_helper).iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 when helper calls mutating method on parameter during live slot loan"
    );

    let src_closure = "\
struct svc {
    count int
}

struct env {
    m map[string]svc
}

fn (e *env) bump() {
    e.m[\"b\"] = svc{count: 2}
}

fn main() {
    e := env{m: map[string]svc{\"a\": svc{count: 1}}}
    s := &e.m[\"a\"]
    f := fn() {
        e.bump()
    }
    f()
    s.count = 99
}
";
    assert!(
        check_codes(src_closure).iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 when local closure calls mutating method on captured receiver during live slot loan"
    );

    let src_returned_closure = "\
struct svc {
    count int
}

struct env {
    m map[string]svc
}

fn (e *env) make_mutator() fn() {
    return fn() {
        e.m[\"b\"] = svc{count: 2}
    }
}

fn main() {
    e := env{m: map[string]svc{\"a\": svc{count: 1}}}
    s := &e.m[\"a\"]
    f := e.make_mutator()
    f()
    s.count = 99
}
";
    assert!(
        check_codes(src_returned_closure)
            .iter()
            .any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 when closure returned by method mutates receiver map during live slot loan"
    );

    let src_readonly_ok = "\
struct svc {
    count int
}

struct env {
    m map[string]svc
}

fn (e *env) size() int {
    return len(e.m)
}

fn helper(e *env) int {
    return e.size()
}

fn main() {
    e := env{m: map[string]svc{\"a\": svc{count: 1}}}
    s := &e.m[\"a\"]
    _ = helper(&e)
    f := fn() int {
        return e.size()
    }
    _ = f()
    s.count = 99
}
";
    assert!(
        !check_codes(src_readonly_ok)
            .iter()
            .any(|c| c == "HYB-E3102"),
        "did not expect HYB-E3102 when helper or closure calls read-only method on receiver"
    );
}
