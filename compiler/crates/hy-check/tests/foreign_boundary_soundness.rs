use hy_check::check_file;

fn check_codes(src: &str) -> Vec<String> {
    let ast = hy_syntax::parse_file("test.hy", src).expect("test source must parse");
    let res = check_file(&ast);
    res.diagnostics.into_iter().map(|d| d.code).collect()
}

#[test]
fn serde_json_new_array_and_push_string_enforce_a2_and_move_rules() {
    let src_a2_method = "\
import \"rust/serde_json\"

fn main() {
    arr := serde_json.new_array()
    view := arr.array(\"\")
    arr.push_string(\"x\")
    _ = view.string(\"\")
}
";
    let codes_a2_method = check_codes(src_a2_method);
    assert!(
        codes_a2_method.iter().any(|c| c == "HYB-E3101"),
        "expected HYB-E3101 for arr.push_string during live borrow view on serde_json.new_array(), got {:?}",
        codes_a2_method
    );

    let src_a2_loop = "\
import \"rust/serde_json\"

fn main() {
    doc := serde_json.parse(`[1, 2]`)?
    for item in doc.array(\"\")? {
        doc.push_string(\"x\")
        _ = item.int(\"\")?
    }
}
";
    let codes_a2_loop = check_codes(src_a2_loop);
    assert!(
        codes_a2_loop.iter().any(|c| c == "HYB-E3101"),
        "expected HYB-E3101 for doc.push_string inside doc.array loop borrow window, got {:?}",
        codes_a2_loop
    );

    let src_a2_pkg = "\
import \"rust/serde_json\"

fn main() {
    doc := serde_json.parse(`[1]`)?
    view := doc.array(\"\")?
    serde_json.push_string(doc, \"b\")
    _ = view.string(\"\")?
}
";
    let codes_a2_pkg = check_codes(src_a2_pkg);
    assert!(
        codes_a2_pkg.iter().any(|c| c == "HYB-E3101"),
        "expected HYB-E3101 for serde_json.push_string during live borrow view, got {:?}",
        codes_a2_pkg
    );

    let src_spawn_unmoved = "\
import \"rust/serde_json\"

fn worker(v serde_json.value) {
    v.push_string(\"x\")
}

fn main() {
    arr := serde_json.new_array()
    task worker(arr)
}
";
    let codes_unmoved = check_codes(src_spawn_unmoved);
    assert!(
        codes_unmoved.iter().any(|c| c == "HYB-E3001"),
        "expected HYB-E3001 when spawning with unmoved serde_json.new_array() handle, got {:?}",
        codes_unmoved
    );

    let src_spawn_moved = "\
import \"rust/serde_json\"

fn worker(v serde_json.value) {
    v.push_string(\"x\")
}

fn main() {
    arr := serde_json.new_array()
    task worker(move arr)
}
";
    let codes_moved = check_codes(src_spawn_moved);
    assert!(
        codes_moved.is_empty(),
        "expected 0 diagnostics when moving serde_json.new_array() handle into task, got {:?}",
        codes_moved
    );
}

#[test]
fn foreign_package_mutated_args_reject_const_writes_with_e3006() {
    let src_json = "\
import \"encoding/json\"

struct cfg {
    count int
}

const global_cfg = cfg{count: 1}

fn main() {
    p := &global_cfg
    _ = json.unmarshal(`{}`, p)
}
";
    let codes_json = check_codes(src_json);
    assert!(
        codes_json.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for json.unmarshal into const struct borrow, got {:?}",
        codes_json
    );

    let src_atomic = "\
import \"sync/atomic\"

const global_arr [2]int64 = [10, 20]

fn main() {
    p := &global_arr
    atomic.store_int64(p, 99)
}
";
    let codes_atomic = check_codes(src_atomic);
    assert!(
        codes_atomic.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for atomic.store_int64 into const array borrow, got {:?}",
        codes_atomic
    );

    let src_hex = "\
import \"encoding/hex\"

const global_dst = [1, 2, 3, 4]

fn main() {
    src := [10, 20]
    _ = hex.encode(global_dst, src)
}
";
    let codes_hex = check_codes(src_hex);
    assert!(
        codes_hex.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for hex.encode into const destination slice, got {:?}",
        codes_hex
    );

    let src_serde_const = "\
import \"rust/serde_json\"

fn mutate_const() {
    const doc = serde_json.new_array()
    serde_json.push_string(doc, \"x\")
}

fn main() {
    mutate_const()
}
";
    let codes_serde = check_codes(src_serde_const);
    assert!(
        codes_serde.iter().any(|c| c == "HYB-E3006"),
        "expected HYB-E3006 for serde_json.push_string on const handle, got {:?}",
        codes_serde
    );
}
