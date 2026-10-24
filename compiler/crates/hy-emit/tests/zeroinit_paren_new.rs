use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit_src(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parses");
    emit_go(&ast).expect("emits")
}

#[test]
fn n5_new_peels_parenthesized_type_arguments_for_structs_and_struct_aliases() {
    let go = emit_src(
        "\
type tally = map[string]int

struct bag {
    label  string
    counts map[string]int
}

type bag_alias = bag

struct shelf {
    top bag
    n   int
}

struct plain {
    n int
}

fn main() {
    p1 := new((bag))
    p2 := new(((shelf)))
    p3 := new((bag_alias))
    p4 := new((plain))
    p5 := new(tally)
    p6 := new((tally))
    fmt.println(p1, p2, p3, p4, p5, p6)
}
",
    );
    assert!(
        go.contains("p1 := &bag{counts: map[string]int{}}"),
        "expected `new((bag))` to N5-allocate map fields, got:\n{go}"
    );
    assert!(
        go.contains("p2 := &shelf{top: bag{counts: map[string]int{}}}"),
        "expected `new(((shelf)))` to N5-allocate nested map fields, got:\n{go}"
    );
    assert!(
        go.contains("p3 := &bag_alias{counts: map[string]int{}}"),
        "expected `new((bag_alias))` struct alias to N5-allocate `&bag_alias{{...}}`, got:\n{go}"
    );
    assert!(
        go.contains("p4 := new(plain)"),
        "expected map-free `new((plain))` to remain `new(plain)`, got:\n{go}"
    );
    assert!(
        go.contains("p5 := new(tally)") && go.contains("p6 := new(tally)"),
        "expected map alias `new(tally)` / `new((tally))` not to emit invalid `&tally{{}}`, got:\n{go}"
    );
}

#[test]
fn n5_new_does_not_rewrite_parenthesized_shadowing_local_var() {
    let go = emit_src(
        "\
struct bag {
    counts map[string]int
}

fn demo() {
    bag := 1
    _ = bag
    _ = new((bag))
}
",
    );
    assert!(
        go.contains("_ = new(bag)"),
        "expected shadowed local `bag` in `new((bag))` not to rewrite to `&bag{{...}}`, got:\n{go}"
    );
    assert!(
        !go.contains("&bag{"),
        "must not emit `&bag{{...}}` when `bag` is a local variable:\n{go}"
    );
}
