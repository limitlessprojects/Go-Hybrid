use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("t.hy", src).expect("must parse");
    check_file(&ast).diagnostics.iter().map(|d| d.code.clone()).collect()
}

#[test]
fn const_init_accepts_complex_ascriptions_and_chained_local_consts() {
    let ok = r#"type my_c128 = complex128
const c1 complex128 = complex(1.0, 2.0)
const c2 complex64 = complex(f32(1.0), f32(2.0))
const c3 my_c128 = c1
const r0 float64 = real(c1)

fn main() {
    const z = complex(1.0, 2.0)
    const r = real(z)
    const im = imag(z)
    const a = i32(1)
    const b = a + 2
    const c64 complex64 = complex(f32(1.0), f32(2.0))
    const re64 = real(c64)
    fmt.println(c2, c3, r0, r, im, b, re64)
}
"#;
    assert!(codes(ok).is_empty(), "{:?}", codes(ok));

    for bad in [
        "fn main() {\n    const s []int = nil\n    _ = s\n}\n",
        "fn main() {\n    var x = 1\n    const y = x\n    _ = y\n}\n",
    ] {
        assert!(codes(bad).contains(&"HYB-E6003".to_string()), "{bad}");
    }
}
