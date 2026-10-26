use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|d| panic!("parse failed: {:?}", d));
    check_file(&ast)
}

#[test]
fn send_and_select_move_record_via_move_for_same_stmt_uses() {
    let r_send_read = check(
        "fn main() {\n\
           m := map[string]int{\"k\": 0}\n\
           chans := []chan map[string]int{make(chan map[string]int, 1)}\n\
           chans[m[\"k\"]] <- move m\n\
         }",
    );
    assert_eq!(
        r_send_read.verdict,
        Verdict::Reject,
        "{:?}",
        r_send_read.diagnostics
    );
    assert!(r_send_read
        .diagnostics
        .iter()
        .any(|d| d.code == "HYB-E3003"));

    let r_send_alias_read = check(
        "fn main() {\n\
           m := map[string]int{\"k\": 0}\n\
           m2 := m\n\
           chans := []chan map[string]int{make(chan map[string]int, 1)}\n\
           chans[m2[\"k\"]] <- move m\n\
         }",
    );
    assert_eq!(
        r_send_alias_read.verdict,
        Verdict::Reject,
        "{:?}",
        r_send_alias_read.diagnostics
    );
    assert!(r_send_alias_read
        .diagnostics
        .iter()
        .any(|d| d.code == "HYB-E3003"));

    let r_select_double_move = check(
        "fn main() {\n\
           m := map[string]int{\"k\": 0}\n\
           ch1 := make(chan map[string]int, 1)\n\
           ch2 := make(chan map[string]int, 1)\n\
           select {\n\
             ch1 <- move m: {}\n\
             ch2 <- move m: {}\n\
           }\n\
         }",
    );
    assert_eq!(
        r_select_double_move.verdict,
        Verdict::Reject,
        "{:?}",
        r_select_double_move.diagnostics
    );
    assert!(r_select_double_move
        .diagnostics
        .iter()
        .any(|d| d.code == "HYB-E3003"));

    let r_select_alias_double_move = check(
        "fn main() {\n\
           m := map[string]int{\"k\": 0}\n\
           m2 := m\n\
           ch1 := make(chan map[string]int, 1)\n\
           ch2 := make(chan map[string]int, 1)\n\
           select {\n\
             ch1 <- move m: {}\n\
             ch2 <- move m2: {}\n\
           }\n\
         }",
    );
    assert_eq!(
        r_select_alias_double_move.verdict,
        Verdict::Reject,
        "{:?}",
        r_select_alias_double_move.diagnostics
    );
    assert!(r_select_alias_double_move
        .diagnostics
        .iter()
        .any(|d| d.code == "HYB-E3003"));

    let r_select_move_then_read = check(
        "fn main() {\n\
           m := map[string]int{\"k\": 0}\n\
           ch1 := make(chan map[string]int, 1)\n\
           ch2 := make(chan int, 1)\n\
           select {\n\
             ch1 <- move m: {}\n\
             ch2 <- m[\"k\"]: {}\n\
           }\n\
         }",
    );
    assert_eq!(
        r_select_move_then_read.verdict,
        Verdict::Reject,
        "{:?}",
        r_select_move_then_read.diagnostics
    );
    assert!(r_select_move_then_read
        .diagnostics
        .iter()
        .any(|d| d.code == "HYB-E3003"));

    let r_select_read_then_move = check(
        "fn main() {\n\
           m := map[string]int{\"k\": 0}\n\
           chans := []chan int{make(chan int, 1)}\n\
           ch2 := make(chan map[string]int, 1)\n\
           select {\n\
             <-chans[m[\"k\"]]: {}\n\
             ch2 <- move m: {}\n\
           }\n\
         }",
    );
    assert_eq!(
        r_select_read_then_move.verdict,
        Verdict::Reject,
        "{:?}",
        r_select_read_then_move.diagnostics
    );
    assert!(r_select_read_then_move
        .diagnostics
        .iter()
        .any(|d| d.code == "HYB-E3003"));
}

#[test]
fn send_select_and_task_move_field_chain_track_index_subexpr_uses() {
    let r_send_moved_idx = check(
        "import \"rust/serde_json\"\n\
         struct slot { doc serde_json.value }\n\
         struct holder { items [1]slot }\n\
         fn worker(idx map[string]int) { idx[\"k\"] = 1 }\n\
         fn main() {\n\
           idx := map[string]int{\"k\": 0}\n\
           ch1 := make(chan serde_json.value, 1)\n\
           h := holder{items: [1]slot{slot{doc: serde_json.new_array()}}}\n\
           task worker(move idx)\n\
           ch1 <- move h.items[idx[\"k\"]].doc\n\
         }",
    );
    assert_eq!(
        r_send_moved_idx.verdict,
        Verdict::Reject,
        "{:?}",
        r_send_moved_idx.diagnostics
    );
    assert!(
        r_send_moved_idx
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3003"),
        "{:?}",
        r_send_moved_idx.diagnostics
    );

    let r_select_moved_idx = check(
        "import \"rust/serde_json\"\n\
         struct slot { doc serde_json.value }\n\
         struct holder { items [1]slot }\n\
         fn main() {\n\
           idx := map[string]int{\"k\": 0}\n\
           ch0 := make(chan map[string]int, 1)\n\
           ch1 := make(chan serde_json.value, 1)\n\
           h := holder{items: [1]slot{slot{doc: serde_json.new_array()}}}\n\
           select {\n\
             ch0 <- move idx: {}\n\
             ch1 <- move h.items[idx[\"k\"]].doc: {}\n\
           }\n\
         }",
    );
    assert_eq!(
        r_select_moved_idx.verdict,
        Verdict::Reject,
        "{:?}",
        r_select_moved_idx.diagnostics
    );
    assert!(
        r_select_moved_idx
            .diagnostics
            .iter()
            .any(|d| d.code == "HYB-E3003"),
        "{:?}",
        r_select_moved_idx.diagnostics
    );
}
