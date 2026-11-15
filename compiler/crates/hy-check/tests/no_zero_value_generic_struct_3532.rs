use hy_check::{check_file, Verdict};

#[test]
fn generic_struct_instantiated_with_no_zero_value_enum_emits_e4002_3532() {
    let base = "enum msg { data(int)\n ok }\nstruct holder[t] { item t }\nstruct outer { h holder[msg] }\ntype arr_box = holder[[2]msg]\n";
    for decl in ["var h holder[msg]", "var b arr_box", "var o outer"] {
        let src = format!("{base}fn main() {{ {decl}\n _ = 1 }}\n");
        let r = check_file(&hy_syntax::parse_file("test.hy", &src).unwrap());
        assert_eq!(r.verdict, Verdict::Reject, "{decl}: {:?}", r.diagnostics);
        assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E4002"), "{decl}: {:?}", r.diagnostics);
    }
    let ok = check_file(&hy_syntax::parse_file("test.hy", &format!("{base}fn main() {{ var h holder[int]\n _ = h.item }}\n")).unwrap());
    assert_eq!(ok.verdict, Verdict::Accept, "{:?}", ok.diagnostics);
}