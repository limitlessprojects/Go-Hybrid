use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("parses");
    check_file(&ast)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn assert_code(src: &str, want: &str) {
    let cs = codes(src);
    assert!(
        !cs.is_empty() && cs.iter().all(|c| c == want),
        "expected only [{want}], got {cs:?} for:\n{src}"
    );
}

fn assert_ok(src: &str) {
    let cs = codes(src);
    assert!(
        cs.is_empty(),
        "expected 0 diagnostics, got {cs:?} for:\n{src}"
    );
}

#[test]
fn shadowed_import_by_param_or_local_rejects_unused_import_e1002() {
    // Parameter `time` shadows `import "time"` — the imported package is never used.
    assert_code(
        "\
import \"time\"
fn f(time int) int {
    return time
}
",
        "HYB-E1002",
    );

    // Local short declaration `strings := 1` shadows `import "strings"`.
    assert_code(
        "\
import \"strings\"
fn f() int {
    strings := 1
    return strings
}
",
        "HYB-E1002",
    );

    // Local `var time int` assigned via `time = 1` shadows `import "time"`.
    assert_code(
        "\
import \"time\"
fn f() int {
    var time int
    time = 1
    return time
}
",
        "HYB-E1002",
    );
}

#[test]
fn fnlit_and_fndecl_named_results_shadow_outer_defines_and_reject_same_scope_redefine() {
    // Reading named result `res` inside inner closure must NOT mark outer unused `res := 1` as used.
    assert_code(
        "\
fn outer() int {
    res := 1
    f := fn() (res int) {
        res = 2
        return res
    }
    return f()
}
",
        "HYB-E1002",
    );

    // Top-scope `res := 1` inside a function or closure with named result `(res int)` declares no new variable.
    assert_code(
        "\
fn f() (res int) {
    res := 1
    return res
}
",
        "HYB-E1002",
    );
}

#[test]
fn select_recv_and_iterate_duplicate_or_write_only_bindings_reject_e1002() {
    assert_code(
        "\
fn f(ch chan int) int {
    select {
    v, v := <-ch:
        return v
    }
}
",
        "HYB-E1002",
    );

    assert_code(
        "\
fn f(xs []int) int {
    for v, v in xs {
        return v
    }
    return 0
}
",
        "HYB-E1002",
    );

    // Write-only iterate binding (`k = 1` with zero reads) cannot be elided to `_`
    // and fails Go compilation as `k declared and not used`.
    assert_code(
        "\
fn f(xs []int) int {
    for k, v in xs {
        k = 1
        return v
    }
    return 0
}
",
        "HYB-E1002",
    );
    // Write-only match pattern binding (`v = 2` with zero reads) cannot be elided
    // and fails Go compilation as `v declared and not used`.
    assert_code(
        "\
fn f(opt option[int]) int {
    match opt {
        some(v): {
            v = 2
            return 1
        }
        none: {
            return 0
        }
    }
}
",
        "HYB-E1002",
    );
}

#[test]
fn positive_controls_used_imports_and_distinct_select_recv_accept() {
    assert_ok(
        "\
import \"strings\"
fn f(ch chan int) string {
    select {
    v, ok := <-ch:
        if ok && v > 0 {
            return strings.repeat(\"a\", v)
        }
        return \"\"
    }
}
",
    );

    assert_ok(
        "\
fn f(opt option[int]) int {
    match opt {
        some(_): {
            return 1
        }
        none: {
            return 0
        }
    }
}
",
    );
}
