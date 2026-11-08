use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit_src(src: &str) -> String {
    emit_go(&parse_file("test.hy", src).expect("parses")).expect("emits")
}

#[test]
fn n5_array_iife_loop_var_does_not_shadow_user_struct_i() {
    let go = emit_src("struct i { m map[string]int }\nfn main() {\n    var arr [2]i\n    arr[0].m[\"k\"] = 1\n    b := [2]i{}\n    b[1].m[\"v\"] = 2\n    fmt.println(arr[0].m[\"k\"] + b[1].m[\"v\"])\n}\n");
    assert!(go.contains("var arr [2]i = func() (hyArr [2]i) { for hyIdx := range hyArr { hyArr[hyIdx] = i{m: map[string]int{}} }; return hyArr }()"));
    assert!(!go.contains("for i := range hyArr"));
}

#[test]
fn n5_new_array_and_parenthesized_new_callee_allocate_maps() {
    let go = emit_src("struct bag { counts map[string]int }\ntype pair = [2]bag\nfn main() {\n    p1 := new([2]bag{})\n    p2 := new(pair)\n    p3 := (new)(bag)\n    p4 := ((new))(([2]bag{}))\n    fmt.println(len(p1) + len(p2) + len(p3.counts) + len(p4))\n}\n");
    for want in [
        "p1 := func() *[2]bag { var hyArr [2]bag; for hyIdx := range hyArr { hyArr[hyIdx] = bag{counts: map[string]int{}} }; return &hyArr }()",
        "p2 := func() *pair { var hyArr pair; for hyIdx := range hyArr { hyArr[hyIdx] = bag{counts: map[string]int{}} }; return &hyArr }()",
        "p3 := &bag{counts: map[string]int{}}",
        "p4 := func() *[2]bag { var hyArr [2]bag; for hyIdx := range hyArr { hyArr[hyIdx] = bag{counts: map[string]int{}} }; return &hyArr }()",
    ] {
        assert!(go.contains(want), "missing {want:?} in:\n{go}");
    }
    assert!(!go.contains("&func()"));
}

#[test]
fn n5_addr_of_empty_array_literal_and_alias_emits_pointer_iife() {
    let go = emit_src("struct bag { counts map[string]int }\ntype pair = [2]bag\nfn main() {\n    p1 := &[2]bag{}\n    p2 := &([2]bag{})\n    p3 := &pair{}\n    s := []*[2]bag{{}}\n    fmt.println(len(p1) + len(p2) + len(p3) + len(s))\n}\n");
    for want in [
        "p1 := func() *[2]bag { var hyArr [2]bag; for hyIdx := range hyArr { hyArr[hyIdx] = bag{counts: map[string]int{}} }; return &hyArr }()",
        "p2 := func() *[2]bag { var hyArr [2]bag; for hyIdx := range hyArr { hyArr[hyIdx] = bag{counts: map[string]int{}} }; return &hyArr }()",
        "p3 := func() *pair { var hyArr pair; for hyIdx := range hyArr { hyArr[hyIdx] = bag{counts: map[string]int{}} }; return &hyArr }()",
        "s := []*[2]bag{func() *[2]bag { var hyArr [2]bag; for hyIdx := range hyArr { hyArr[hyIdx] = bag{counts: map[string]int{}} }; return &hyArr }()}",
    ] {
        assert!(go.contains(want), "missing {want:?} in:\n{go}");
    }
    assert!(!go.contains("&func()"));
}

#[test]
fn n5_partial_array_literals_with_hex_octal_binary_and_const_lengths_pad_elements() {
    let go = emit_src("struct bag { counts map[string]int }\nconst k = 3\nfn main() {\n    hex_arr := [0x2]bag{bag{}}\n    oct_arr := [0o3]bag{bag{}}\n    bin_arr := [0b10]bag{bag{}}\n    const_arr := [k]bag{bag{}}\n    ptr_const := &[k]bag{bag{}}\n    fmt.println(len(hex_arr) + len(oct_arr) + len(bin_arr) + len(const_arr) + len(ptr_const))\n}\n");
    for want in [
        "hex_arr := [0x2]bag{bag{counts: map[string]int{}}, bag{counts: map[string]int{}}}",
        "oct_arr := [0o3]bag{bag{counts: map[string]int{}}, bag{counts: map[string]int{}}, bag{counts: map[string]int{}}}",
        "bin_arr := [0b10]bag{bag{counts: map[string]int{}}, bag{counts: map[string]int{}}}",
        "const_arr := func() (hyArr [k]bag) { hyArr = [k]bag{bag{counts: map[string]int{}}}; for hyIdx := 1; hyIdx < len(hyArr); hyIdx++ { hyArr[hyIdx] = bag{counts: map[string]int{}} }; return hyArr }()",
        "ptr_const := func() *[k]bag { hyArr := func() (hyArr [k]bag) { hyArr = [k]bag{bag{counts: map[string]int{}}}; for hyIdx := 1; hyIdx < len(hyArr); hyIdx++ { hyArr[hyIdx] = bag{counts: map[string]int{}} }; return hyArr }(); return &hyArr }()",
    ] {
        assert!(go.contains(want), "missing {want:?} in:\n{go}");
    }
}

#[test]
fn n5_type_alias_and_elided_array_composite_literals_allocate_elements() {
    let go = emit_src("struct bag { counts map[string]int }\ntype pair = [2]bag\nfn main() {\n    p := pair{}\n    grid := [][2]bag{{}}\n    alias_grid := []pair{{}}\n    m := map[string][2]bag{\"k\": {}}\n    fmt.println(len(p) + len(grid) + len(alias_grid) + len(m))\n}\n");
    for want in [
        "p := func() (hyArr pair) { for hyIdx := range hyArr { hyArr[hyIdx] = bag{counts: map[string]int{}} }; return hyArr }()",
        "grid := [][2]bag{func() (hyArr [2]bag) { for hyIdx := range hyArr { hyArr[hyIdx] = bag{counts: map[string]int{}} }; return hyArr }()}",
        "alias_grid := []pair{func() (hyArr pair) { for hyIdx := range hyArr { hyArr[hyIdx] = bag{counts: map[string]int{}} }; return hyArr }()}",
        "m := map[string][2]bag{\"k\": func() (hyArr [2]bag) { for hyIdx := range hyArr { hyArr[hyIdx] = bag{counts: map[string]int{}} }; return hyArr }()}",
    ] {
        assert!(go.contains(want), "missing {want:?} in:\n{go}");
    }
}
