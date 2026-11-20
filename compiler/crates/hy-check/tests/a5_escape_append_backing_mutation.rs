fn check_codes(src: &str) -> Vec<String> {
    let file = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    hy_check::check_file(&file).diagnostics.into_iter().map(|d| d.code).collect()
}

fn assert_rejects_e3013(src: &str) {
    let codes = check_codes(src);
    assert!(codes.iter().any(|c| c == "HYB-E3013"), "expected HYB-E3013 for:\n{src}\ngot: {codes:?}");
}

fn assert_accepts(src: &str) {
    let codes = check_codes(src);
    assert!(codes.is_empty(), "expected 0 diagnostics for:\n{src}\ngot: {codes:?}");
}

const PRE: &str = "import \"net/http\"\nimport \"time\"\nstruct slice_box { xs []int }\nfn _use_pkgs() {\n    _ = time.second\n    http.handle_func(\"/k\", fn() {})\n}\nfn push_into(buf []int) {\n    _ = append(buf, 9)\n}\nfn copy_arr(a [4]int) {\n    _ = append(a[:0], 9)\n}\nfn (b slice_box) push_zero() {\n    _ = append(b.xs[:0], 9)\n}";

#[test]
fn a5_escape_rejects_in_place_append_on_captured_backing_storage() {
    for body in [
        "xs := make([]int, 0, 4)\n_ = time.after_func(0, fn() { _ = append(xs, 1) })\n_ = xs",
        "xs := []int{1, 2, 3, 4}\n_ = time.after_func(0, fn() { _ = append(xs[0:0], 9) })\n_ = xs",
        "xs := []int{1, 2, 3, 4}\n_ = time.after_func(0, fn() {\nys := xs[:1]\n_ = append(ys, 9)\n})\n_ = xs",
        "b := slice_box{xs: []int{1, 2}}\n_ = time.after_func(0, fn() { _ = append(b.xs[:1], 9) })\n_ = b",
        "xs := []int{1, 2, 3, 4}\n_ = time.after_func(0, fn() { push_into(xs[:1]) })\n_ = xs",
        "b := slice_box{xs: []int{1, 2}}\nhttp.handle_func(\"/a\", b.push_zero)\n_ = b",
        "arr := [4]int{1, 2, 3, 4}\n_ = time.after_func(0, fn() { _ = append(arr[:0], 9) })\n_ = arr",
        "const xs = []int{1, 2, 3}\n_ = time.after_func(0, fn() { _ = append(xs[:1], 9) })\n_ = xs",
    ] {
        assert_rejects_e3013(&format!("{PRE}\nfn main() {{\n{body}\n}}\n"));
    }
}

#[test]
fn a5_escape_accepts_non_mutating_or_managed_append() {
    for body in [
        "_ = time.after_func(0, fn() {\nlocal := make([]int, 0, 4)\nlocal = append(local, 1)\n_ = local\n})",
        "const xs = []int{1, 2}\n_ = time.after_func(0, fn() {\nout := append(xs, 3)\n_ = out\n})",
        "v := 7\n_ = time.after_func(0, fn() {\nout := append([]int{1}, v)\n_ = out\n})",
        "arr := [4]int{1, 2, 3, 4}\n_ = time.after_func(0, fn() { copy_arr(arr) })\n_ = arr",
        "xs := []int{1, 2, 3, 4}\nmanaged(\"timer sync\", \"interop\", \"#1\") {\n_ = time.after_func(0, fn() { _ = append(xs[0:0], 9) })\n}\n_ = xs",
    ] {
        assert_accepts(&format!("{PRE}\nfn main() {{\n{body}\n}}\n"));
    }
}
