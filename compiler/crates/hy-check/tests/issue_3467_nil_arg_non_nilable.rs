use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("test.hy", src).expect("snippet parses");
    check_file(&ast)
}

fn error_codes(res: &hy_check::CheckResult) -> Vec<String> {
    res.diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn test_direct_fn_call_nil_non_nilable_rejected() {
    let snippets = [
        // int parameter
        r#"
package main
fn takes_int(x int32) {}
fn main() {
    takes_int(nil)
}
"#,
        // string parameter
        r#"
package main
fn takes_str(s string) {}
fn main() {
    takes_str(nil)
}
"#,
        // bool parameter
        r#"
package main
fn takes_bool(b bool) {}
fn main() {
    takes_bool(nil)
}
"#,
        // float parameter
        r#"
package main
fn takes_float(f float64) {}
fn main() {
    takes_float(nil)
}
"#,
        // struct parameter
        r#"
package main
struct point {
    x int32
}
fn takes_struct(p point) {}
fn main() {
    takes_struct(nil)
}
"#,
        // array parameter
        r#"
package main
fn takes_array(a [3]int32) {}
fn main() {
    takes_array(nil)
}
"#,
    ];

    for src in snippets {
        let res = check(src);
        assert_eq!(
            res.verdict,
            Verdict::Reject,
            "expected reject for nil passed to non-nilable parameter, got accept:\n{src}"
        );
        let codes = error_codes(&res);
        assert!(
            codes.contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 in {:?} for snippet:\n{src}",
            codes
        );
    }
}

#[test]
fn test_method_call_nil_non_nilable_rejected() {
    let snippets = [
        // value receiver method
        r#"
package main
struct point {
    x int32
}
fn (p point) set_x(v int32) {}
fn main() {
    var p point
    p.set_x(nil)
}
"#,
        // pointer receiver method
        r#"
package main
struct point {
    x int32
}
fn (p *point) set_x(v int32) {}
fn main() {
    var p point
    p.set_x(nil)
}
"#,
        // method expression
        r#"
package main
struct point {
    x int32
}
fn (p point) set_x(v int32) {}
fn main() {
    var p point
    point.set_x(p, nil)
}
"#,
    ];

    for src in snippets {
        let res = check(src);
        assert_eq!(
            res.verdict,
            Verdict::Reject,
            "expected reject for nil passed to method parameter, got accept:\n{src}"
        );
        let codes = error_codes(&res);
        assert!(
            codes.contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 in {:?} for snippet:\n{src}",
            codes
        );
    }
}

#[test]
fn test_closure_call_nil_non_nilable_rejected() {
    let snippets = [
        // closure variable
        r#"
package main
fn main() {
    var f = fn(x int32) {}
    f(nil)
}
"#,
        // immediate closure literal call
        r#"
package main
fn main() {
    (fn(s string) {})(nil)
}
"#,
    ];

    for src in snippets {
        let res = check(src);
        assert_eq!(
            res.verdict,
            Verdict::Reject,
            "expected reject for nil passed to closure parameter, got accept:\n{src}"
        );
        let codes = error_codes(&res);
        assert!(
            codes.contains(&"HYB-E1002".to_string()),
            "expected HYB-E1002 in {:?} for snippet:\n{src}",
            codes
        );
    }
}

#[test]
fn test_positive_nil_to_nilable_parameters_accepted() {
    let snippets = [
        // pointer parameter
        r#"
package main
fn takes_ptr(p *int32) {}
fn main() {
    takes_ptr(nil)
}
"#,
        // slice parameter
        r#"
package main
fn takes_slice(s []int32) {}
fn main() {
    takes_slice(nil)
}
"#,
        // map parameter
        r#"
package main
fn takes_map(m map[string]int32) {}
fn main() {
    takes_map(nil)
}
"#,
        // chan parameter
        r#"
package main
fn takes_chan(ch chan int32) {}
fn main() {
    takes_chan(nil)
}
"#,
        // interface (any) parameter
        r#"
package main
fn takes_any(a any) {}
fn main() {
    takes_any(nil)
}
"#,
        // function parameter
        r#"
package main
fn takes_fn(f fn()) {}
fn main() {
    takes_fn(nil)
}
"#,
    ];

    for src in snippets {
        let res = check(src);
        assert_eq!(
            res.verdict,
            Verdict::Accept,
            "expected accept for nil passed to nilable parameter, got {:?}:\n{src}",
            res.diagnostics
        );
        assert!(
            res.diagnostics.is_empty(),
            "expected 0 diagnostics, got {:?}",
            res.diagnostics
        );
    }
}

