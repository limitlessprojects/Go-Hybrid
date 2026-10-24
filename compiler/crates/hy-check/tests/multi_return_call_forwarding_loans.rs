use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check_codes(src: &str) -> (Verdict, Vec<String>) {
    let parsed = parse_file("inline.hy", src).expect("must parse");
    let res = check_file(&parsed);
    let codes = res.diagnostics.iter().map(|d| d.code.clone()).collect();
    (res.verdict, codes)
}

#[test]
fn rejects_const_mutation_via_forwarded_multi_return_call() {
    let src = r#"
const shared = map[string]int{"k": 1}
fn pair_with_tag(m map[string]int) (int, map[string]int) { return 0, m }
fn write_second(tag int, m map[string]int) { m["k"] = tag + 99 }
fn main() { write_second(pair_with_tag(shared)) }
"#;
    let (verdict, codes) = check_codes(src);
    assert_eq!(verdict, Verdict::Reject);
    assert!(codes.iter().any(|c| c == "HYB-E3006"), "{:?}", codes);
}

#[test]
fn rejects_const_mutation_via_forwarded_method_and_fnlit() {
    let src = r#"
const shared = map[string]int{"k": 1}
struct writer { id int }
fn (w writer) write_second(tag int, m map[string]int) { m["k"] = w.id + tag }
fn pair_const() (int, map[string]int) { return 7, shared }
fn main() {
    w := writer{id: 1}
    w.write_second(pair_const())
    (fn(tag int, m map[string]int) { m["k"] = tag })(pair_const())
}
"#;
    let (verdict, codes) = check_codes(src);
    assert_eq!(verdict, Verdict::Reject);
    assert_eq!(
        codes.iter().filter(|c| c.as_str() == "HYB-E3006").count(),
        2,
        "{:?}",
        codes
    );
}

#[test]
fn rejects_out_param_store_via_forwarded_multi_return_call() {
    let src = r#"
fn pair_maps(dst map[string][]int, src []int) (map[string][]int, []int) { return dst, src }
fn link_slot(dst map[string][]int, src []int) { dst["k"] = src }
fn consume(xs []int) { xs[0] = 99 }
fn main() {
    dst := map[string][]int{}
    src := []int{1, 2, 3}
    link_slot(pair_maps(dst, src))
    task consume(move src)
    dst["k"][0] = 42
}
"#;
    let (verdict, codes) = check_codes(src);
    assert_eq!(verdict, Verdict::Reject);
    assert!(codes.iter().any(|c| c == "HYB-E3003"), "{:?}", codes);
}

#[test]
fn rejects_a3_slot_loan_invalidation_via_forwarded_multi_return_call() {
    let src = r#"
struct entry { v int }
fn pair_entry_map(tag int, m map[string]entry) (int, map[string]entry) { return tag, m }
fn insert_second(tag int, m map[string]entry) { m["b"] = entry{v: tag} }
fn main() {
    m := map[string]entry{"a": entry{v: 1}}
    slot := &m["a"]
    insert_second(pair_entry_map(2, m))
    slot.v = 99
}
"#;
    let (verdict, codes) = check_codes(src);
    assert_eq!(verdict, Verdict::Reject);
    assert!(codes.iter().any(|c| c == "HYB-E3102"), "{:?}", codes);
}

#[test]
fn accepts_read_only_forwarded_multi_return_call() {
    let src = r#"
const shared = map[string]int{"k": 1}
fn pair_with_tag(m map[string]int) (int, map[string]int) { return 1, m }
fn read_second(tag int, m map[string]int) int { return tag + m["k"] }
fn main() { _ = read_second(pair_with_tag(shared)) }
"#;
    let (verdict, codes) = check_codes(src);
    assert_eq!(verdict, Verdict::Accept, "{:?}", codes);
}
