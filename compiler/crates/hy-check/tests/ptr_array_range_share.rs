use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("inline.hy", src).expect("must parse");
    check_file(&ast)
        .diagnostics
        .into_iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code)
        .collect()
}

#[test]
fn range_over_addr_of_array_tracks_may_share_across_move_spawn() {
    let src = r#"
fn worker(s []int) {
    s[0] = 1
}

fn main() {
    inner := []int{0}
    arr := [1][]int{inner}
    var leaked []int = []int{}
    for _, elem in &arr {
        leaked = elem
    }
    task worker(move inner)
    leaked[0] = 2
}
"#;
    let got = codes(src);
    assert!(
        got.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 for post-move write through alias extracted via `&arr` range, got {got:?}"
    );
}

#[test]
fn range_over_ptr_to_array_binding_tracks_may_share_across_move_spawn() {
    let src = r#"
fn worker(s []int) {
    s[0] = 1
}

fn main() {
    inner := []int{0}
    arr := [1][]int{inner}
    p := &arr
    var leaked []int = []int{}
    for _, elem in p {
        leaked = elem
    }
    task worker(move inner)
    leaked[0] = 2
}
"#;
    let got = codes(src);
    assert!(
        got.iter().any(|c| c == "HYB-E3003"),
        "expected HYB-E3003 for post-move write through alias extracted via `*[N][]int` binding range, got {got:?}"
    );
}

#[test]
fn range_over_ptr_to_array_rejects_unmoved_slice_elem_at_spawn() {
    let src = r#"
fn worker(s []int) {
    s[0] = 1
}

fn main() {
    arr := [1][]int{[]int{0}}
    p := &arr
    for _, elem in p {
        task worker(elem)
    }
}
"#;
    let got = codes(src);
    assert!(
        got.iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 when spawning task with slice element from `*[N][]int` range, got {got:?}"
    );
}

#[test]
fn range_over_addr_of_primitive_array_allows_copy_spawn() {
    let src = r#"
fn worker(v int) {
    _ = v
}

fn main() {
    arr := [2]int{10, 20}
    for _, v in &arr {
        task worker(v)
    }
}
"#;
    let got = codes(src);
    assert!(
        got.is_empty(),
        "expected 0 diagnostics when spawning primitive element from `&[2]int`, got {got:?}"
    );
}
