use hy_check::{check_file, Verdict};
#[test]
fn place_target_ty_resolves_generic_structs_and_type_aliases_for_e6002_3548() {
    let base = "struct holder[t] { val t\n items []t\n m map[t]int }\ntype byte_slice = []u8\ntype byte_box = holder[u8]\n";
    for assign in ["var h holder[u8]\n h.val = 300\n _ = h", "var h holder[u8]\n h.items[0] = 300\n _ = h", "var h holder[u8]\n h.m[300] = 1\n _ = h", "var b byte_box\n b.val = 300\n _ = b", "var xs byte_slice\n xs[0] = 300\n _ = xs"] {
        let r = check_file(&hy_syntax::parse_file("test.hy", &format!("{base}fn main() {{ {assign} }}\n")).unwrap());
        assert!(r.verdict == Verdict::Reject && r.diagnostics.iter().any(|d| d.code == "HYB-E6002"), "{assign}: {:?}", r.diagnostics);
    }
}
