use hy_emit::emit_go;
use hy_syntax::parse_file;

#[test]
fn binding_kind_preserved_across_generic_assoc_try_and_new_3685() {
    let src = "import \"fmt\"\nstruct box {}\nfn box.make_set() set[int] {\n    return set[int]{}\n}\nfn box.make_set_g[t comparable]() set[t] {\n    return set[t]{}\n}\nfn make_set[t comparable]() set[t] {\n    return set[t]{}\n}\nfn make_set_err[t comparable]() (set[t], error) {\n    return set[t]{}, nil\n}\nfn make_map[k comparable, v any]() map[k]v {\n    return map[k]v{}\n}\nfn make_chan[t any]() chan t {\n    return make(chan t, 1)\n}\nfn main() {\n    s1 := make_set[int]()\n    s2 := box.make_set()\n    s3 := box.make_set_g[int]()\n    s4 := (make_set_err[int]())?\n    p := new(set[int])\n    delete(p, 1)\n    for x in p {\n        fmt.println(x)\n    }\n    for k in make_map[string, int]() {\n        fmt.println(k)\n    }\n    for v in make_chan[int]() {\n        fmt.println(v)\n    }\n    fmt.println(len(s1), len(s2), len(s3), len(s4), len(p))\n}\n";
    let ast = parse_file("test.hy", src).expect("parses");
    let go = emit_go(&ast).expect("emits");
    for want in [
        "p.remove(1)",
        "for x := range p.m {",
        "for k := range make_map[string, int]() {",
        "for v := range make_chan[int]() {",
        "fmt.Println(s1.len(), s2.len(), s3.len(), s4.len(), p.len())",
    ] {
        assert!(go.contains(want), "missing {want:?} in:\n{go}");
    }
}
