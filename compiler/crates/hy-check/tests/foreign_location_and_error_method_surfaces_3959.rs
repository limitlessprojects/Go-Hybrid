//! Row 13: validate foreign methods against the receiver's static Go type.
use hy_check::{check_file, Verdict};

fn assert_call(src: &str, missing: Option<&str>) {
    let ast = hy_syntax::parse_file("test.hy", src).expect("fixture must parse");
    let r = check_file(&ast);
    if let Some(method) = missing {
        assert_eq!(r.verdict, Verdict::Reject, "{src}\n{:?}", r.diagnostics);
        assert!(
            r.diagnostics
                .iter()
                .any(|d| d.code == "HYB-E1002" && d.message.contains(method)),
            "expected HYB-E1002 for `{method}`: {src}\n{:?}",
            r.diagnostics
        );
    } else {
        assert_eq!(r.verdict, Verdict::Accept, "{src}\n{:?}", r.diagnostics);
    }
}

#[test]
fn declared_receivers_expose_their_go_methods() {
    let cases: &[(&str, &str, &[&str], &str)] = &[
        ("time", "location", &["string"], "reset"),
        ("time", "parse_error", &["error"], "unwrap"),
        ("strconv", "num_error", &["error", "unwrap"], "timeout"),
        ("os", "path_error", &["error", "timeout", "unwrap"], "string"),
        ("os", "link_error", &["error", "unwrap"], "timeout"),
        ("os", "syscall_error", &["error", "timeout", "unwrap"], "string"),
    ];
    for &(pkg, ty, methods, missing) in cases {
        let field = |method: &str| {
            format!(
                "import \"{pkg}\"\nstruct holder {{\n    item {pkg}.{ty}\n}}\n\
                 fn main() {{\n    var h holder\n    h.item.{method}()\n}}\n"
            )
        };
        let pointer = |method: &str| {
            format!("import \"{pkg}\"\nfn inspect(x *{pkg}.{ty}) {{\n    x.{method}()\n}}\n")
        };
        for &method in methods {
            assert_call(&field(method), None);
            assert_call(&pointer(method), None);
        }
        assert_call(&field(missing), Some(missing));
        assert_call(&pointer(missing), Some(missing));
    }
}

#[test]
fn inferred_locations_reject_reset() {
    let src = |method: &str| {
        format!(
            "import \"time\"\nfn main() {{\n    loc := time.fixed_zone(\"UTC+1\", 3600)\n\
             \x20   loc.{method}()\n}}\n"
        )
    };
    assert_call(&src("string"), None);
    assert_call(&src("reset"), Some("reset"));
}

#[test]
fn new_syscall_error_exposes_only_the_error_interface() {
    let src = |method: &str| {
        format!(
            "import \"os\"\nfn inspect(cause error) {{\n\
             \x20   e := os.new_syscall_error(\"read\", cause)\n    e.{method}()\n}}\n"
        )
    };
    assert_call(&src("error"), None);
    assert_call(&src("unwrap"), Some("unwrap"));
    assert_call(&src("timeout"), Some("timeout"));
}
