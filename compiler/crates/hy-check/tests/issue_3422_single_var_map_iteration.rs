use hy_check::{check_file, Verdict};
use hy_syntax::parse_file;

#[test]
fn test_single_var_map_iteration_shape_and_move_repro_3422() {
    let src = r#"
struct box { val int }

fn worker(p *box, done chan int) {
  for i in 0..5000 {
    p.val = i
  }
  done <- 1
}

fn main() {
  done := make(chan int, 1)
  b := &box{val: 0}
  m := map[*box]int{b: 1}
  for k in m {
    alias := k
    task worker(move alias, done)
    for j in 0..5000 {
      k.val = 99
    }
  }
  _ = <-done
}
"#;
    let ast = parse_file("test.hy", src).expect("parses");
    let res = check_file(&ast);

    // Must be rejected with use-after-move (E3003) or move-on-gc-field (E3004)
    assert_eq!(
        res.verdict,
        Verdict::Reject,
        "expected single-variable map iteration over map[*box]int to reject concurrent move"
    );
}
