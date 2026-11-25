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
fn buildable_rejects_composite_array_out_of_bounds_index_and_slice_3694() {
    for src in [
        "import \"fmt\"\nfn main() {\n    var a [2][3]int\n    fmt.println(a[1][3])\n}\n",
        "import \"fmt\"\nfn main() {\n    var a [2][3][4]int\n    fmt.println(a[1][2][4])\n}\n",
        "fn main() {\n    var a [2][3]int\n    a[1][3] = 1\n    _ = a\n}\n",
        "fn main() {\n    var a [2][3]int\n    a[1][3] += 1\n    _ = a\n}\n",
        "import \"fmt\"\nstruct s_t { a [3]int }\nfn main() {\n    var s s_t\n    fmt.println(s.a[3])\n}\n",
        "import \"fmt\"\nstruct s_t { a [3]int }\nfn main() {\n    var s [2]s_t\n    fmt.println(s[0].a[3])\n}\n",
        "import \"fmt\"\nfn f() [3]int {\n    return [3]int{1, 2, 3}\n}\nfn main() {\n    fmt.println(f()[3])\n}\n",
        "import \"fmt\"\nfn main() {\n    var s [][3]int = make([][3]int, 2)\n    fmt.println(s[0][3])\n}\n",
        "import \"fmt\"\nfn main() {\n    var a [2][3]int\n    p := &a\n    fmt.println(p[1][3])\n}\n",
        "import \"fmt\"\nfn main() {\n    var a [2][3]int\n    fmt.println(a[1][1:5])\n}\n",
        "import \"fmt\"\nfn main() {\n    var m map[string][3]int\n    fmt.println(m[\"x\"][3])\n}\n",
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
struct s_t { a [3]int }
fn f() [3]int { return [3]int{1, 2, 3} }
fn main() {
    var a [2][3]int
    a[1][2] = 9
    p := &a
    var s s_t
    var sa [2]s_t
    var sl [][3]int = make([][3]int, 2)
    var m map[string][3]int = map[string][3]int{\"x\": [3]int{1, 2, 3}}
    fmt.println(a[1][2], p[1][2], a[1][1:3], s.a[2], sa[0].a[2], f()[2], sl[0][2], m[\"x\"][2])
}
";
    assert!(codes(ok).is_empty(), "{:?}", codes(ok));
}
