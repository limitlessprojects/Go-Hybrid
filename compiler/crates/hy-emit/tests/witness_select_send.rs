use hy_emit::emit_go_with_proof;
use hy_syntax::parse_file;

#[test]
fn select_send_arm_records_witness_and_determinism() {
    let s1 = "package main\n\nfn main() {\n    ch := make(chan int, 1)\n    select {\n        ch <- 42: {}\n        _ := <-ch: {}\n        default: {}\n    }\n}\n";
    let (go, pj) = emit_go_with_proof(&parse_file("test.hy", s1).unwrap()).unwrap();
    assert!(go.contains("case ch <- 42:"));
    let p: serde_json::Value = serde_json::from_str(&pj).unwrap();
    assert_eq!((p["task_sites"].as_u64(), p["send_sites"].as_u64()), (Some(0), Some(1)));
    let ws = p["witnesses"].as_array().unwrap();
    assert_eq!(ws.len(), 1);
    let (w, h) = (&ws[0], ws[0]["node_hash"].as_str().unwrap());
    assert_eq!(w["site"], "send");
    assert!(h.len() == 16 && h.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    assert_eq!((w["span"]["start"]["line"].as_u64(), w["span"]["start"]["col"].as_u64(), w["span"]["end"]["line"].as_u64(), w["span"]["end"]["col"].as_u64()), (Some(6), Some(9), Some(6), Some(17)));

    let s2 = "package main\nfn worker(ch chan int) {}\nfn step() (int, error) { return 10, nil }\nfn run() error {\n    ch := make(chan int, 2)\n    ch2 := make(chan int, 2)\n    a := 5\n    task worker(ch)\n    ch <- 1\n    select {\n        ch <- move a: {}\n        ch2 <- step()?: {}\n    }\n    return nil\n}\nfn main() {}\n";
    let (go1, pj1) = emit_go_with_proof(&parse_file("test_concurrency.hy", s2).unwrap()).unwrap();
    let p2: serde_json::Value = serde_json::from_str(&pj1).unwrap();
    assert_eq!((p2["task_sites"].as_u64(), p2["send_sites"].as_u64()), (Some(1), Some(3)));
    let ws2 = p2["witnesses"].as_array().unwrap();
    assert_eq!(ws2.iter().map(|w| w["site"].as_str().unwrap()).collect::<Vec<_>>(), vec!["task", "send", "send", "send"]);
    assert!(ws2.iter().all(|w| w["node_hash"].as_str().is_some_and(|h| h.len() == 16 && h.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()))));
    let (go2, pj2) = emit_go_with_proof(&parse_file("test_concurrency.hy", s2).unwrap()).unwrap();
    assert_eq!((go1, pj1), (go2, pj2));
}
