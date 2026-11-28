use hy_check::{check_file, CheckResult, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast =
        parse_file("inline.hy", src).unwrap_or_else(|diags| panic!("must parse: {:?}", diags));
    check_file(&ast)
}

fn error_codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_rejects_e1002(src: &str) {
    let r = check(src);
    let codes = error_codes(&r);
    assert_eq!(r.verdict, Verdict::Reject, "expected reject for:\n{src}");
    assert!(
        codes.iter().any(|c| c == "HYB-E1002"),
        "expected HYB-E1002 for:\n{src}\ngot: {:?}",
        r.diagnostics
    );
}

fn assert_accepts(src: &str) {
    let r = check(src);
    assert_eq!(
        r.verdict,
        Verdict::Accept,
        "expected accept for:\n{src}\ngot: {:?}",
        r.diagnostics
    );
}

#[test]
fn explicit_comparable_struct_rejects_incomparable_type_args() {
    for src in [
        "struct holder[t comparable] { v t }\nfn main() {\n    h := holder[[]int]{v: []int{1}}\n    _ = h\n}\n",
        "struct holder[t comparable] { v t }\nfn main() {\n    var h holder[map[string]int]\n    _ = h\n}\n",
        "struct holder[t comparable] { v t }\nfn check_fn(h holder[fn()]) {\n    _ = h\n}\nfn main() {}\n",
        "struct holder[t comparable] { v t }\ntype bad = holder[[2][]int]\nfn main() {}\n",
        "struct holder[t comparable] { v t }\nfn main() {\n    p := new(holder[[]int])\n    _ = p\n}\n",
        "struct holder[t comparable] { v t }\nstruct box[u] { inner u }\nfn main() {\n    var b box[holder[[]int]]\n    _ = b\n}\n",
    ] {
        assert_rejects_e1002(src);
    }
}

#[test]
fn field_implied_and_transitive_comparable_structs_reject_incomparable_args() {
    for src in [
        "struct cache[k, v] { m map[k]v }\nfn main() {\n    var c cache[[]int, int]\n    _ = c\n}\n",
        "struct arr_key[k] { m map[[2]k]int }\nfn main() {\n    var c arr_key[[]int]\n    _ = c\n}\n",
        "struct seen[t] { s set[t] }\nfn main() {\n    var s seen[map[string]int]\n    _ = s\n}\n",
        "struct inner[k] { m map[k]int }\nstruct outer[t] { in_val inner[t] }\nfn main() {\n    var o outer[[]int]\n    _ = o\n}\n",
    ] {
        assert_rejects_e1002(src);
    }
}

#[test]
fn comparable_and_unconstrained_generic_struct_usages_stay_accepted() {
    let src = "\
struct pt { x int }
struct holder[t comparable] { v t }
struct box[t] { v t }
struct cache[k, v] { m map[k]v }

fn forward[t comparable](h holder[t]) holder[t] {
    return h
}

fn main() {
    a := holder[int]{v: 1}
    b := holder[pt]{v: pt{x: 2}}
    var c holder[*[]int]
    var d holder[chan map[string]int]
    e := box[[]int]{v: []int{1}}
    var f cache[string, []int]
    g := forward[int](a)
    _ = b
    _ = c
    _ = d
    _ = e
    _ = f
    _ = g
}
";
    assert_accepts(src);
}
