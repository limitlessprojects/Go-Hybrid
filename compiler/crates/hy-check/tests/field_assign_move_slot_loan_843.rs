use hy_check::check_file;
use hy_syntax::parse_file;

#[test]
fn field_and_index_assign_propagates_move_slot_loans_and_const_issue_843() {
    let src = "struct Entry { n int }\nstruct Holder { m map[string]Entry }\nfn main() {\n m := map[string]Entry{\"a\": Entry{n: 1}}\n var h Holder\n e := &m[\"a\"]\n h.m = move m\n h.m[\"b\"] = Entry{n: 2}\n e.n = 99\n}\n";
    let res = check_file(&parse_file("test.hy", src).expect("parse"));
    assert!(res.diagnostics.iter().any(|d| d.code == "HYB-E3102"), "got {:?}", res.diagnostics);
}
