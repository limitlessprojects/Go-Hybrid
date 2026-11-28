use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let file = parse_file("test.hy", src).expect("parse ok");
    check_file(&file)
        .diagnostics
        .into_iter()
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn move_field_chain_index_use_after_move_rejected() {
    let src = r#"
import "rust/serde_json"

struct holder {
    conn serde_json.value
}

fn main() {
    idxs := [0, 1]
    _ = move idxs
    var holders []holder
    c := move holders[len(idxs)].conn
    _ = c
}
"#;
    let got = codes(src);
    assert!(
        got.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 for moved binding read in index of `move holders[len(idxs)].conn`, got {:?}",
        got
    );
}

#[test]
fn move_field_chain_slice_use_after_move_rejected() {
    let src = r#"
import "rust/serde_json"

struct holder {
    conn serde_json.value
}

fn main() {
    bounds := [0, 1]
    _ = move bounds
    var holders []holder
    c := move holders[0:len(bounds)][0].conn
    _ = c
}
"#;
    let got = codes(src);
    assert!(
        got.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 for moved binding read in slice bound of `move holders[0:len(bounds)][0].conn`, got {:?}",
        got
    );
}

#[test]
fn move_field_chain_index_extends_map_slot_loan_last_use() {
    let src = r#"
import "rust/serde_json"

struct slot {
    idx int
}

struct holder {
    conn serde_json.value
}

fn main() {
    m := map[string]slot{"a": slot{idx: 0}}
    p := &m["a"]
    m["b"] = slot{idx: 1}
    var holders []holder
    c := move holders[p.idx].conn
    _ = c
}
"#;
    let got = codes(src);
    assert!(
        got.iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 when map is mutated before `p.idx` is read in `move holders[p.idx].conn`, got {:?}",
        got
    );
}

#[test]
fn closure_capture_after_map_write_extends_map_slot_loan_last_use() {
    let src = r#"
struct acct {
    bal int
}

fn main() {
    m := map[string]acct{"a": acct{bal: 10}}
    p := &m["a"]
    m["b"] = acct{bal: 20}
    f := fn() int {
        return p.bal
    }
    _ = f()
}
"#;
    let got = codes(src);
    assert!(
        got.iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 when map is mutated before a closure capturing slot loan `p` runs, got {:?}",
        got
    );
}

#[test]
fn closure_capture_extends_borrow_handle_loan_last_use() {
    let src = r#"
import "rust/serde_json"

fn main() {
    doc := serde_json.new_object()
    v := doc.array("k")?
    f := fn() int {
        return len(v)
    }
    doc.set("k", "x")?
    _ = f()
}
"#;
    let got = codes(src);
    assert!(
        got.iter().any(|c| c == "HYB-E3101"),
        "expected HYB-E3101 when `doc.set` mutates `doc` while borrow view `v` is live in closure `f`, got {:?}",
        got
    );
}

#[test]
fn for_iterate_call_mutating_const_param_rejected() {
    let src = r#"
import "sort"

fn sort_and_ret(xs []int) []int {
    sort.ints(xs)
    return xs
}

fn iter_sorted(xs []int) {
    for x in sort_and_ret(xs) {
        _ = x
    }
}

fn main() {
    const nums = [3, 1, 2]
    iter_sorted(nums)
}
"#;
    let got = codes(src);
    assert!(
        got.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 when helper mutates const slice inside `for x in sort_and_ret(xs)`, got {:?}",
        got
    );
}

#[test]
fn closure_for_iterate_call_mutating_map_invalidates_slot_loan() {
    let src = r#"
struct acct {
    bal int
}

fn put_and_keys(m map[string]acct) []string {
    m["b"] = acct{bal: 2}
    return ["a"]
}

fn main() {
    f := fn() int {
        m := map[string]acct{"a": acct{bal: 1}}
        p := &m["a"]
        for k in put_and_keys(m) {
            _ = k
        }
        return p.bal
    }
    _ = f()
}
"#;
    let got = codes(src);
    assert!(
        got.iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 when closure `for k in put_and_keys(m)` mutates map while slot loan `p` is live, got {:?}",
        got
    );
}
