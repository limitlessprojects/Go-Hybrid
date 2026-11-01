use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|e| panic!("parse failed: {:?}", e));
    check_file(&ast)
}

#[test]
fn assign_place_field_and_nested_index_calls_enforce_const_write_and_a3() {
    let r_const = check(
        "struct item { v int }\n\
         fn mut_map(m map[string]int) string {\n\
           m[\"k\"] = 1\n\
           return \"a\"\n\
         }\n\
         fn main() {\n\
           const c = map[string]int{\"k\": 0}\n\
           var m = map[string]item{\"a\": item{v: 1}}\n\
           m[mut_map(c)].v = 2\n\
         }",
    );
    assert_eq!(r_const.verdict, Verdict::Reject);
    assert!(
        r_const.diagnostics.iter().any(|d| d.code == "HYB-E3006"),
        "expected HYB-E3006 for const-mutating call inside field-projected assign place index, got {:?}",
        r_const.diagnostics
    );

    let r_a3 = check(
        "struct item { v int }\n\
         fn insert_key(m map[string]item) int {\n\
           m[\"b\"] = item{v: 9}\n\
           return 0\n\
         }\n\
         fn main() {\n\
           var m = map[string]item{\"a\": item{v: 1}}\n\
           var arr = [1]item{item{v: 0}}\n\
           p := &m[\"a\"]\n\
           arr[insert_key(m)].v = 5\n\
           fmt.println(p.v)\n\
         }",
    );
    assert_eq!(r_a3.verdict, Verdict::Reject);
    assert!(
        r_a3.diagnostics.iter().any(|d| d.code == "HYB-E3102"),
        "expected HYB-E3102 for map-inserting call inside field-projected assign place index during live slot loan, got {:?}",
        r_a3.diagnostics
    );
}

#[test]
fn assign_place_index_closure_literal_enforces_boundary_sendability() {
    let r = check(
        "fn worker(m map[string]int) { m[\"k\"] = 1 }\n\
         fn main() {\n\
           m := map[string]int{\"k\": 0}\n\
           var arr = [1]int{0}\n\
           arr[(fn() int {\n\
             task worker(m)\n\
             return 0\n\
           })()] = 7\n\
         }",
    );
    assert_eq!(r.verdict, Verdict::Reject);
    assert!(
        r.diagnostics.iter().any(|d| d.code == "HYB-E3001"),
        "expected HYB-E3001 for closure literal inside assign target index spawning shared map, got {:?}",
        r.diagnostics
    );
}

#[test]
fn move_projection_index_and_slice_bounds_enforce_use_after_move_and_a3_loan_window() {
    let r_uam = check(
        "struct box_idx { i int }\n\
         struct holder { h serde_json.value }\n\
         fn main() {\n\
           b := box_idx{i: 0}\n\
           ch := make(chan box_idx, 1)\n\
           ch <- move b\n\
           v := serde_json.new_object()\n\
           arr := [1]holder{holder{h: move v}}\n\
           out := make(chan serde_json.value, 1)\n\
           out <- move arr[b.i].h\n\
         }",
    );
    assert_eq!(r_uam.verdict, Verdict::Reject);
    assert!(
        r_uam.diagnostics.iter().any(|d| d.code == "HYB-E3003"),
        "expected HYB-E3003 for moved binding read inside move projection index, got {:?}",
        r_uam.diagnostics
    );

    let r_loan = check(
        "struct slot_idx { i int }\n\
         struct holder { h serde_json.value }\n\
         fn main() {\n\
           m := map[string]slot_idx{\"a\": slot_idx{i: 0}}\n\
           p := &m[\"a\"]\n\
           m[\"b\"] = slot_idx{i: 1}\n\
           v := serde_json.new_object()\n\
           arr := [1]holder{holder{h: move v}}\n\
           out := make(chan serde_json.value, 1)\n\
           out <- move arr[p.i].h\n\
         }",
    );
    assert_eq!(r_loan.verdict, Verdict::Reject);
    assert!(
        r_loan.diagnostics.iter().any(|d| d.code == "HYB-E3102"),
        "expected HYB-E3102 when map-slot loan is read in move projection index after map insert, got {:?}",
        r_loan.diagnostics
    );
}

#[test]
fn o8_slot_key_normalizes_raw_escaped_and_concat_string_keys() {
    let r_raw = check(
        "struct item { v int }\n\
         fn main() {\n\
           m := map[string]item{\"a\": item{v: 1}}\n\
           p := &m[\"a\"]\n\
           q := &m[`a`]\n\
           fmt.println(p.v, q.v)\n\
         }",
    );
    assert_eq!(r_raw.verdict, Verdict::Reject);
    assert!(
        r_raw.diagnostics.iter().any(|d| d.code == "HYB-E3102"),
        "expected HYB-E3102 for raw string key `a` aliasing cooked key \"a\" slot loan, got {:?}",
        r_raw.diagnostics
    );

    let r_concat = check(
        "struct item { v int }\n\
         fn main() {\n\
           m := map[string]item{\"ab\": item{v: 1}}\n\
           p := &m[\"ab\"]\n\
           q := &m[\"a\" + \"b\"]\n\
           fmt.println(p.v, q.v)\n\
         }",
    );
    assert_eq!(r_concat.verdict, Verdict::Reject);
    assert!(
        r_concat.diagnostics.iter().any(|d| d.code == "HYB-E3102"),
        "expected HYB-E3102 for concatenated string key \"a\" + \"b\" aliasing \"ab\" slot loan, got {:?}",
        r_concat.diagnostics
    );

    let r_distinct = check(
        "struct item { v int }\n\
         fn main() {\n\
           m := map[string]item{\"a\": item{v: 1}, \"b\": item{v: 2}}\n\
           p := &m[\"a\"]\n\
           q := &m[`b`]\n\
           fmt.println(p.v, q.v)\n\
         }",
    );
    assert_eq!(r_distinct.verdict, Verdict::Accept, "got {:?}", r_distinct.diagnostics);
}
