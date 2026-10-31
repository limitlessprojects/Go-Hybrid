#[test]
fn composite_float_rejects_typed_int_variables() {
    for s in [
        "fn main() { i := 10\ns := []float64{i}\n_ = s }\n",
        "fn main() { i := 10\nm := map[float64]string{i: \"v\"}\n_ = m }\n",
        "fn main() { i := 10\nm := map[string]float64{\"k\": i}\n_ = m }\n",
    ] {
        let r = hy_check::check_file(&hy_syntax::parse_file("t.hy", s).unwrap());
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
            "expected HYB-E1002 in diagnostics: {:?}",
            r.diagnostics
        );
    }
}

#[test]
fn composite_float_accepts_untyped_int_constants() {
    for s in [
        "fn main() { s := []float64{10, 20}\n_ = s }\n",
        "fn main() { m := map[float64]string{10: \"v\"}\n_ = m }\n",
        "fn main() { m := map[string]float64{\"k\": 10}\n_ = m }\n",
    ] {
        let r = hy_check::check_file(&hy_syntax::parse_file("t.hy", s).unwrap());
        assert!(
            !r.diagnostics.iter().any(|d| d.code == "HYB-E1002"),
            "unexpected HYB-E1002 in diagnostics: {:?}",
            r.diagnostics
        );
    }
}
