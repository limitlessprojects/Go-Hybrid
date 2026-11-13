use hy_emit::emit_go_with_proof;
use hy_syntax::parse_file;

#[test]
fn select_send_arm_records_witness() {
    let src = r#"package main

fn main() {
    ch := make(chan int, 1)
    select {
        ch <- 42: {}
        _ := <-ch: {}
        default: {}
    }
}
"#;
    let file = parse_file("test.hy", src).expect("parse succeeds");
    let (go, proof_json) = emit_go_with_proof(&file).expect("emit succeeds");
    assert!(go.contains("case ch <- 42:"));

    let proof: serde_json::Value =
        serde_json::from_str(&proof_json).expect("proof is valid JSON");
    assert_eq!(proof["task_sites"], 0);
    assert_eq!(proof["send_sites"], 1);

    let witnesses = proof["witnesses"].as_array().expect("witnesses array");
    assert_eq!(witnesses.len(), 1);

    let w = &witnesses[0];
    assert_eq!(w["site"], "send");

    let hash = w["node_hash"].as_str().expect("string node_hash");
    assert_eq!(hash.len(), 16);
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));

    let start_line = w["span"]["start"]["line"].as_u64().unwrap();
    let start_col = w["span"]["start"]["col"].as_u64().unwrap();
    let end_line = w["span"]["end"]["line"].as_u64().unwrap();
    let end_col = w["span"]["end"]["col"].as_u64().unwrap();
    assert_eq!((start_line, start_col), (6, 9));
    assert_eq!((end_line, end_col), (6, 17));
}

#[test]
fn combined_concurrency_sites_emission_order_and_determinism() {
    let src = r#"package main

fn worker(ch chan int) {}
fn step() (int, error) { return 10, nil }

fn run() error {
    ch := make(chan int, 2)
    ch2 := make(chan int, 2)
    a := 5
    task worker(ch)
    ch <- 1
    select {
        ch <- move a: {}
        ch2 <- step()?: {}
    }
    return nil
}

fn main() {}
"#;
    let file = parse_file("test_concurrency.hy", src).expect("parse succeeds");
    let (go1, proof_json1) = emit_go_with_proof(&file).expect("emit succeeds");

    let proof: serde_json::Value =
        serde_json::from_str(&proof_json1).expect("proof is valid JSON");
    assert_eq!(proof["task_sites"], 1);
    assert_eq!(proof["send_sites"], 3);

    let witnesses = proof["witnesses"].as_array().expect("witnesses array");
    assert_eq!(witnesses.len(), 4);

    let sites: Vec<&str> = witnesses
        .iter()
        .map(|w| w["site"].as_str().unwrap())
        .collect();
    assert_eq!(sites, vec!["task", "send", "send", "send"]);

    for w in witnesses {
        let hash = w["node_hash"].as_str().expect("valid hash string");
        assert_eq!(hash.len(), 16);
        assert!(hash.chars().all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
    }

    let file2 = parse_file("test_concurrency.hy", src).expect("re-parse succeeds");
    let (go2, proof_json2) = emit_go_with_proof(&file2).expect("re-emit succeeds");
    assert_eq!(go1, go2);
    assert_eq!(proof_json1, proof_json2);
}
