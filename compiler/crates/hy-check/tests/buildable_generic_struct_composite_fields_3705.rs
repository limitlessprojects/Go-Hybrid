use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let f = parse_file("test.hy", src).expect("parses");
    check_file(&f)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

#[test]
fn buildable_checks_generic_struct_array_map_and_option_field_kinds() {
    for src in [
        // 1. Generic struct instantiated with [3]int: non-integer index on b.val["bad"]
        "struct box[t] { val t }\nfn main() {\n    b := box[[3]int]{ val: [3]int{1, 2, 3} }\n    _ = b.val[\"bad\"]\n}\n",
        // 2. Generic struct instantiated with [3]int: mismatched element type var s string = b.val[0]
        "struct box[t] { val t }\nfn main() {\n    b := box[[3]int]{ val: [3]int{1, 2, 3} }\n    var s string = b.val[0]\n    _ = s\n}\n",
        // 3. Generic struct instantiated with [3]int: mismatched array length var a [2]int = b.val
        "struct box[t] { val t }\nfn main() {\n    b := box[[3]int]{ val: [3]int{1, 2, 3} }\n    var a [2]int = b.val\n    _ = a\n}\n",
        // 4. Generic struct instantiated with map[string]int: wrong key type b.val[123]
        "struct box[t] { val t }\nfn main() {\n    b := box[map[string]int]{ val: map[string]int{\"a\": 1} }\n    _ = b.val[123]\n}\n",
        // 5. Generic struct instantiated with map[i32]int: wrong key scalar width b.val[k64]
        "struct box[t] { val t }\nfn main() {\n    b := box[map[i32]int]{ val: map[i32]int{} }\n    var k i64 = 1\n    _ = b.val[k]\n}\n",
        // 6. Generic struct instantiated with option[int]: mismatched default argument b.val.or(\"bad\")
        "struct box[t] { val t }\nfn main() {\n    b := box[option[int]]{ val: some(1) }\n    _ = b.val.or(\"bad\")\n}\n",
        // 7. Generic struct instantiated with []i64: element scalar width mismatch on b.val[0] = x32
        "struct box[t] { val t }\nfn main() {\n    b := box[[]i64]{ val: []i64{1} }\n    var x i32 = 2\n    b.val[0] = x\n    _ = b\n}\n",
    ] {
        let got = codes(src);
        assert!(
            got.iter().any(|c| c == "HYB-E1002"),
            "expected HYB-E1002, got {:?} for:\n{src}",
            got
        );
    }

    let ok = "\
import \"fmt\"
struct box[t] { val t }
fn main() {
    ba := box[[3]int]{ val: [3]int{1, 2, 3} }
    var a [3]int = ba.val
    bm := box[map[string]int]{ val: map[string]int{\"a\": 7} }
    bo := box[option[int]]{ val: some(9) }
    bs := box[[]i64]{ val: []i64{10} }
    var v i64 = 20
    bs.val[0] = v
    fmt.println(a[0], bm.val[\"a\"], bo.val.or(0), bs.val[0])
}
";
    assert!(codes(ok).is_empty(), "{:?}", codes(ok));
}
