//! Regression tests for `addressable.rs` (`HYB-E2010`) across closure-bound
//! map parameters/locals/binders, pointer-to-array (`*[N]T`) hops, nested
//! `&m[k].f` / `&m[k][i]` in local/assign value expressions, and composite
//! `.unwrap()` / closure-call map bases.

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|d| panic!("must parse: {d:?}"));
    check_file(&ast)
}

fn has_e2010(src: &str) -> bool {
    check(src)
        .diagnostics
        .iter()
        .any(|d| d.severity == Severity::Error && d.code == "HYB-E2010")
}

#[test]
fn closure_bound_maps_and_sibling_shadows_reject_with_e2010() {
    for (label, src) in [
        (
            "closure map parameter field store `m[\"a\"].bal = 1`",
            "struct acct { bal int }\nfn main() {\n f := fn(m map[string]acct) {\n m[\"a\"].bal = 1\n }\n _ = f\n}\n",
        ),
        (
            "closure map parameter field loan `q := &m[\"a\"].bal`",
            "struct acct { bal int }\nfn main() {\n f := fn(m map[string]acct) {\n q := &m[\"a\"].bal\n fmt.println(\"{q}\")\n }\n _ = f\n}\n",
        ),
        (
            "closure struct-with-map parameter `b.accts[\"a\"].bal = 1`",
            "struct acct { bal int }\nstruct bank { accts map[string]acct }\nfn main() {\n f := fn(b bank) {\n b.accts[\"a\"].bal = 1\n }\n _ = f\n}\n",
        ),
        (
            "closure-local map compound store and loan",
            "struct acct { bal int }\nfn main() {\n f := fn() {\n m := map[string]acct{\"a\": acct{bal: 1}}\n m[\"a\"].bal += 1\n }\n f()\n}\n",
        ),
        (
            "closure `for` iterate binder map element store",
            "struct acct { bal int }\nfn main() {\n f := fn(ms []map[string]acct) {\n for _, m in ms {\n m[\"a\"].bal++\n }\n }\n _ = f\n}\n",
        ),
        (
            "closure `match` arm binder map element store",
            "struct acct { bal int }\nfn main() {\n f := fn(opt option[map[string]acct]) {\n match opt {\n some(m) => {\n m[\"a\"].bal = 2\n }\n none => {}\n }\n }\n _ = f\n}\n",
        ),
        (
            "closure `select` recv binder map element store",
            "struct acct { bal int }\nfn main() {\n f := fn(ch chan map[string]acct) {\n select {\n m := <-ch:\n {\n m[\"a\"].bal = 2\n }\n default:\n {}\n }\n }\n _ = f\n}\n",
        ),
        (
            "outer map captured by closure with later inner block binding `m`",
            "struct acct { bal int }\nfn main() {\n m := map[string]acct{\"a\": acct{bal: 1}}\n f := fn() {\n m[\"a\"].bal = 3\n if false {\n m := 0\n fmt.println(\"{m}\")\n }\n }\n f()\n}\n",
        ),
    ] {
        assert!(has_e2010(src), "{label} must emit HYB-E2010:\n{src}");
    }

    // Control: closure param/local shadowing an outer map with a slice stays accepted.
    let control = "struct acct { bal int }\nfn main() {\n m := map[string]acct{\"a\": acct{bal: 1}}\n f := fn(m []acct) {\n m[0].bal = 2\n }\n g := fn() {\n m := []acct{acct{bal: 1}}\n m[0].bal = 3\n }\n _ = f\n g()\n fmt.println(\"{m[\"a\"].bal}\")\n}\n";
    assert!(!has_e2010(control), "slice shadow in closure must not emit HYB-E2010");
}

#[test]
fn pointer_to_array_hops_and_nested_or_composite_slot_loans_reject_with_e2010() {
    for (label, src) in [
        (
            "pointer-to-array `p[0][\"a\"].bal = 1` field store",
            "struct acct { bal int }\nfn f(p *[2]map[string]acct) {\n p[0][\"a\"].bal = 1\n}\nfn main() {}\n",
        ),
        (
            "pointer-to-array `p[0][\"a\"][0] = 1` array element store",
            "fn f(p *[2]map[string][2]int) {\n p[0][\"a\"][0] = 1\n}\nfn main() {}\n",
        ),
        (
            "pointer-to-array `q := &p[0][\"a\"].bal` field loan",
            "struct acct { bal int }\nfn f(p *[2]map[string]acct) {\n q := &p[0][\"a\"].bal\n fmt.println(\"{q}\")\n}\nfn main() {}\n",
        ),
        (
            "`move`-wrapped field loan `q := move &m[\"a\"].bal`",
            "struct acct { bal int }\nfn main() {\n m := map[string]acct{\"a\": acct{bal: 1}}\n q := move &m[\"a\"].bal\n fmt.println(\"{q}\")\n}\n",
        ),
        (
            "nested `&m[\"a\"].bal` in call argument of local initializer",
            "struct acct { bal int }\nfn touch(p *int) int { return 0 }\nfn main() {\n m := map[string]acct{\"a\": acct{bal: 1}}\n x := touch(&m[\"a\"].bal)\n fmt.println(\"{x}\")\n}\n",
        ),
        (
            "nested `&m[\"a\"].bal` in slice literal initializer",
            "struct acct { bal int }\nfn main() {\n m := map[string]acct{\"a\": acct{bal: 1}}\n s := []*int{&m[\"a\"].bal}\n fmt.println(\"{len(s)}\")\n}\n",
        ),
        (
            "nested `&m[\"a\"].bal` in assignment target index expression",
            "struct acct { bal int }\nfn idx(p *int) int { return 0 }\nfn main() {\n m := map[string]acct{\"a\": acct{bal: 0}}\n xs := []int{1, 2}\n xs[idx(&m[\"a\"].bal)] = 9\n fmt.println(\"{xs[0]}\")\n}\n",
        ),
        (
            "composite `(<-ch).unwrap()[\"a\"].bal` loan",
            "struct acct { bal int }\nfn f(ch chan option[map[string]acct]) {\n q := &(<-ch).unwrap()[\"a\"].bal\n fmt.println(\"{q}\")\n}\nfn main() {}\n",
        ),
        (
            "local closure call `get_m()[\"a\"].bal` loan",
            "struct acct { bal int }\nfn main() {\n m := map[string]acct{\"a\": acct{bal: 1}}\n get_m := fn() map[string]acct { return m }\n q := &get_m()[\"a\"].bal\n fmt.println(\"{q}\")\n}\n",
        ),
    ] {
        assert!(has_e2010(src), "{label} must emit HYB-E2010:\n{src}");
    }
}
