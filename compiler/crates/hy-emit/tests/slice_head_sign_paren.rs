//! An untyped slice literal whose first item is a signed or parenthesized
//! numeric literal infers its element type like the bare literal does.
//! `[-1, 2]` was refused with `untyped slice literal of `-1` — spell the
//! element type`, though `[1, -2]` emitted fine.

use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> Result<String, String> {
    let ast = parse_file("t.hy", src).expect("parses");
    emit_go(&ast).map_err(|e| format!("{e:?}"))
}

#[test]
fn signed_and_parenthesized_numeric_heads_infer_the_element_type() {
    let go = emit(
        "fn main() { a := [-1, 2]\n b := [(3), 4]\n c := [-2.5, 1]\n d := [-(7)]\n e := [(\"s\")]\n \
         fmt.println(\"{len(a)} {len(b)} {len(c)} {len(d)} {len(e)}\") }\n",
    )
    .expect("emits");
    for want in [
        "[]int{-1, 2}",
        "[]int{(3), 4}",
        "[]float64{-2.5, 1}",
        "[]int{-(7)}",
        "[]string{(\"s\")}",
    ] {
        assert!(go.contains(want), "missing `{want}` in:\n{go}");
    }
}

#[test]
fn a_negated_non_numeric_head_is_still_refused() {
    let err = emit("fn main() { x := 1\n a := [-x]\n fmt.println(\"{len(a)}\") }\n").unwrap_err();
    assert!(err.contains("untyped slice literal"), "{err}");
}
