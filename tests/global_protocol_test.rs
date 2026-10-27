use super::*;

#[test]
fn test_protocol_generation() {
    let protocol = GlobalProtocol {
        participants: vec!["A".to_string(), "B".to_string()],
        channels: vec![
            Channel {
                name: "ch1".to_string(),
                sender: "A".to_string(),
                receiver: "B".to_string(),
                typ: "string".to_string(),
            },
            Channel {
                name: "ch2".to_string(),
                sender: "B".to_string(),
                receiver: "A".to_string(),
                typ: "string".to_string(),
            },
        ],
        choices: vec![
            Choice {
                name: "choice1".to_string(),
                participant: "A".to_string(),
                options: vec!["option1".to_string(), "option2".to_string()],
            },
        ],
    };

    let code = generate_from_protocol(&protocol);
    assert!(code.contains("var ch1 = make(chan string)"));
    assert!(code.contains("var ch2 = make(chan string)"));
    assert!(code.contains("func A() {"));
    assert!(code.contains("func B() {"));
    assert!(code.contains("go A()"));
    assert!(code.contains("go B()"));
}