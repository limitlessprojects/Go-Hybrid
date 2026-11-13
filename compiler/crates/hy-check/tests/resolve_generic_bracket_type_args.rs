use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check_codes(src: &str) -> (Verdict, Vec<String>) {
    let f = parse_file("test.hy", src).expect("parses");
    let r = check_file(&f);
    let codes = r.diagnostics.into_iter().map(|d| d.code).collect();
    (r.verdict, codes)
}

fn assert_rejects_e1001(src: &str) {
    let (v, codes) = check_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E1001"),
        "expected HYB-E1001 for:\n{src}\ngot: {codes:?}"
    );
    assert_eq!(v, Verdict::Reject, "expected Reject for:\n{src}");
}

fn assert_no_e1001(src: &str) {
    let (_, codes) = check_codes(src);
    assert!(
        !codes.iter().any(|c| c == "HYB-E1001"),
        "must NOT fire HYB-E1001 for:\n{src}\ngot: {codes:?}"
    );
}

#[test]
fn generic_call_and_method_bracketed_type_args_reject_undefined_types_e1001() {
    // 1. Generic function call with single, multi, or nested bracketed type args
    assert_rejects_e1001(
        "fn id[t any](x t) t {\n    return x\n}\nfn main() {\n    _ = id[undef_ty](42)\n}\n",
    );
    assert_rejects_e1001(
        "fn pair[a any, b any](x a, y b) a {\n    return x\n}\nfn main() {\n    _ = pair[int, undef_ty](1, 2)\n}\n",
    );
    assert_rejects_e1001(
        "struct box[t any] {\n    v t\n}\nfn id[t any](x t) t {\n    return x\n}\nfn main() {\n    var b box[int]\n    _ = id[box[undef_ty]](b)\n}\n",
    );
    assert_rejects_e1001(
        "fn pair[a any, b any](x a, y b) a {\n    return x\n}\nfn main() {\n    _ = pair[int, []undef_ty](1, nil)\n}\n",
    );
    assert_rejects_e1001(
        "fn pair[a any, b any](x a, y b) a {\n    return x\n}\nfn main() {\n    _ = pair[int, *undef_ty](1, nil)\n}\n",
    );

    // 2. Method / selector call with bracketed type args (`Expr::Field`)
    assert_rejects_e1001(
        "struct holder {\n    v int\n}\nfn (h holder) wrap[t any](x t) int {\n    _ = x\n    return h.v\n}\nfn main() {\n    h := holder{v: 1}\n    _ = h.wrap[undef_ty](1)\n}\n",
    );

    // 3. Local map/slice variable shadowing a file-level fn must still validate index key
    assert_rejects_e1001(
        "fn tbl() {}\nfn run(tbl map[string]fn(int) int) int {\n    return tbl[undef_key](1)\n}\nfn main() {}\n",
    );
}

#[test]
fn new_and_make_type_operands_reject_undefined_inner_types_e1001() {
    assert_rejects_e1001("fn main() {\n    _ = new(set[undef_ty])\n}\n");
    assert_rejects_e1001("fn main() {\n    _ = new(option[undef_ty])\n}\n");
    assert_rejects_e1001(
        "struct box[t any] {\n    v t\n}\nfn main() {\n    _ = new(box[set[undef_ty]])\n}\n",
    );
    assert_rejects_e1001(
        "struct box[t any] {\n    v t\n}\nfn main() {\n    _ = make([]box[[undef_len]int], 1)\n}\n",
    );
    assert_rejects_e1001("fn main() {\n    _ = make(chan set[[undef_len]int], 1)\n}\n");
}

#[test]
fn array_length_inside_generic_named_type_rejects_undefined_const_e1001() {
    assert_rejects_e1001(
        "struct box[t any] {\n    v t\n}\nfn main() {\n    var b box[[undef_len]int]\n    _ = b\n}\n",
    );
    assert_rejects_e1001(
        "struct box[t any] {\n    v t\n}\ntype bad_alias = box[[undef_len]int]\nfn main() {}\n",
    );
    assert_rejects_e1001(
        "fn main() {\n    s := set[[undef_len]int]{}\n    _ = s\n}\n",
    );
}

#[test]
fn valid_generic_calls_new_make_and_const_array_lengths_accept_without_e1001() {
    assert_no_e1001(
        "const n = 4\nstruct box[t any] {\n    v t\n}\nfn id[t any](x t) t {\n    return x\n}\nfn pair[a any, b any](x a, y b) a {\n    return x\n}\nfn (b *box[t]) run(x t) {\n    _ = id[t](x)\n    _ = pair[t, []t](x, make([]t, 1))\n    _ = pair[t, *t](x, new(t))\n    _ = pair[t, box[[n]int]](x, box[[n]int]{v: [n]int{1, 2, 3, 4}})\n    _ = new(set[int])\n    _ = new(option[t])\n    _ = new(box[set[t]])\n    _ = make(chan t, 1)\n}\nfn main() {}\n",
    );
}
