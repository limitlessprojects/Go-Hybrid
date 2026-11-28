use std::collections::BTreeMap;
use w010_session_projection::*;

fn role(s: &str) -> Role { s.to_string() }
fn lbl(s: &str) -> Label { s.to_string() }

// -------- prereg success bar 2: A/B ping-pong must NOT synthesize --------

#[test]
fn ping_pong_is_not_synthesizable() {
    let mut roles: BTreeMap<Role, Local> = BTreeMap::new();
    roles.insert(role("A"), Local::Send {
        to: role("B"), label: lbl("p"),
        cont: Box::new(Local::Recv { from: role("B"), label: lbl("q"), cont: Box::new(Local::End) }),
    });
    roles.insert(role("B"), Local::Send {
        to: role("A"), label: lbl("q"),
        cont: Box::new(Local::Recv { from: role("A"), label: lbl("p"), cont: Box::new(Local::End) }),
    });
    let r = synthesize(&roles);
    assert!(r.is_err(), "prereg bar 2 violated: ping-pong synthesized to {:?}", r);
    match r.unwrap_err() {
        SynthError::NoMatchingReceiver { .. } => {}
        other => panic!("expected NoMatchingReceiver, got {:?}", other),
    }
}

// -------- prereg success bar 3+4: request/reply synthesizes and terminates --------

#[test]
fn request_reply_synthesizes_and_terminates() {
    let mut roles: BTreeMap<Role, Local> = BTreeMap::new();
    roles.insert(role("A"), Local::Send {
        to: role("B"), label: lbl("req"),
        cont: Box::new(Local::Recv { from: role("B"), label: lbl("resp"), cont: Box::new(Local::End) }),
    });
    roles.insert(role("B"), Local::Recv {
        from: role("A"), label: lbl("req"),
        cont: Box::new(Local::Send { to: role("A"), label: lbl("resp"), cont: Box::new(Local::End) }),
    });

    let g = synthesize(&roles).expect("prereg bar 3: request/reply must synthesize");

    // Exact shape
    match &g {
        Global::Msg { from, to, label, cont } => {
            assert_eq!(from, &role("A"));
            assert_eq!(to, &role("B"));
            assert_eq!(label, &lbl("req"));
            match cont.as_ref() {
                Global::Msg { from, to, label, cont: c2 } => {
                    assert_eq!(from, &role("B"));
                    assert_eq!(to, &role("A"));
                    assert_eq!(label, &lbl("resp"));
                    assert!(matches!(c2.as_ref(), Global::End));
                }
                other => panic!("expected second Msg, got {:?}", other),
            }
        }
        other => panic!("expected Msg, got {:?}", other),
    }

    // Round-trip
    let pa = project(&g, &role("A")).unwrap();
    let pb = project(&g, &role("B")).unwrap();
    assert_eq!(pa, roles[&role("A")]);
    assert_eq!(pb, roles[&role("B")]);

    // prereg bar 4: exhaustive simulation terminates
    match simulate_locals(&roles) {
        SimResult::Terminates { states, trace } => {
            assert!(states > 0);
            assert_eq!(trace.len(), 2, "expected exactly 2 events, got {:?}", trace);
        }
        other => panic!("prereg bar 4: expected Terminates, got {:?}", other),
    }
}

// -------- ping-pong simulation must deadlock --------

#[test]
fn ping_pong_simulation_deadlocks() {
    let mut roles: BTreeMap<Role, Local> = BTreeMap::new();
    roles.insert(role("A"), Local::Send {
        to: role("B"), label: lbl("p"),
        cont: Box::new(Local::Recv { from: role("B"), label: lbl("q"), cont: Box::new(Local::End) }),
    });
    roles.insert(role("B"), Local::Send {
        to: role("A"), label: lbl("q"),
        cont: Box::new(Local::Recv { from: role("A"), label: lbl("p"), cont: Box::new(Local::End) }),
    });
    match simulate_locals(&roles) {
        SimResult::Deadlock { stuck, witness, .. } => {
            assert!(!stuck.is_empty());
            assert!(witness.is_empty(), "no event should be firable in ping-pong");
        }
        other => panic!("expected Deadlock, got {:?}", other),
    }
}

// -------- projection of uninvolved role through a choice --------

#[test]
fn project_choice_uninvolved_merge_ok() {
    // A chooses between "x" and "y" toward B; C is uninvolved in both branches
    let g = Global::Choice {
        at: role("A"),
        to: role("B"),
        branches: vec![
            (lbl("x"), Global::Msg { from: role("B"), to: role("C"), label: lbl("m"), cont: Box::new(Global::End) }),
            (lbl("y"), Global::Msg { from: role("B"), to: role("C"), label: lbl("m"), cont: Box::new(Global::End) }),
        ],
    };
    let pc = project(&g, &role("C")).expect("C should project through merged branches");
    match pc {
        Local::Recv { from, label, .. } => {
            assert_eq!(from, role("B"));
            assert_eq!(label, lbl("m"));
        }
        other => panic!("expected Recv, got {:?}", other),
    }
}

// -------- project_choice_uninvolved_diverges must error --------

#[test]
fn project_choice_uninvolved_diverge_errors() {
    let g = Global::Choice {
        at: role("A"),
        to: role("B"),
        branches: vec![
            (lbl("x"), Global::Msg { from: role("B"), to: role("C"), label: lbl("m"), cont: Box::new(Global::End) }),
            (lbl("y"), Global::Msg { from: role("B"), to: role("C"), label: lbl("n"), cont: Box::new(Global::End) }),
        ],
    };
    let r = project(&g, &role("C"));
    assert!(matches!(r, Err(ProjectError::UninvolvedBranchMerge { .. })),
        "expected UninvolvedBranchMerge, got {:?}", r);
}

// -------- three-role pipeline synthesizes and terminates --------

#[test]
fn three_role_pipeline() {
    let mut roles: BTreeMap<Role, Local> = BTreeMap::new();
    roles.insert(role("A"), Local::Send {
        to: role("B"), label: lbl("m1"),
        cont: Box::new(Local::Recv { from: role("C"), label: lbl("m3"), cont: Box::new(Local::End) }),
    });
    roles.insert(role("B"), Local::Recv {
        from: role("A"), label: lbl("m1"),
        cont: Box::new(Local::Send { to: role("C"), label: lbl("m2"), cont: Box::new(Local::End) }),
    });
    roles.insert(role("C"), Local::Recv {
        from: role("B"), label: lbl("m2"),
        cont: Box::new(Local::Send { to: role("A"), label: lbl("m3"), cont: Box::new(Local::End) }),
    });
    let g = synthesize(&roles).expect("three-role pipeline should synthesize");
    // Round-trip all
    for r in ["A", "B", "C"] {
        assert_eq!(project(&g, &role(r)).unwrap(), roles[&role(r)]);
    }
    match simulate_locals(&roles) {
        SimResult::Terminates { trace, .. } => assert_eq!(trace.len(), 3),
        other => panic!("expected Terminates, got {:?}", other),
    }
}

// -------- three-role cross-cycle rejects and deadlocks --------

#[test]
fn three_role_cross_cycle() {
    let mut roles: BTreeMap<Role, Local> = BTreeMap::new();
    roles.insert(role("A"), Local::Send {
        to: role("B"), label: lbl("a"),
        cont: Box::new(Local::Recv { from: role("C"), label: lbl("c"), cont: Box::new(Local::End) }),
    });
    roles.insert(role("B"), Local::Send {
        to: role("C"), label: lbl("b"),
        cont: Box::new(Local::Recv { from: role("A"), label: lbl("a"), cont: Box::new(Local::End) }),
    });
    roles.insert(role("C"), Local::Send {
        to: role("A"), label: lbl("c"),
        cont: Box::new(Local::Recv { from: role("B"), label: lbl("b"), cont: Box::new(Local::End) }),
    });
    assert!(synthesize(&roles).is_err(), "cross-cycle must not synthesize");
    match simulate_locals(&roles) {
        SimResult::Deadlock { stuck, .. } => assert_eq!(stuck.len(), 3),
        other => panic!("expected Deadlock, got {:?}", other),
    }
}

// -------- codegen produces non-empty Go --------

#[test]
fn codegen_nonempty() {
    let mut roles: BTreeMap<Role, Local> = BTreeMap::new();
    roles.insert(role("A"), Local::Send {
        to: role("B"), label: lbl("req"),
        cont: Box::new(Local::Recv { from: role("B"), label: lbl("resp"), cont: Box::new(Local::End) }),
    });
    roles.insert(role("B"), Local::Recv {
        from: role("A"), label: lbl("req"),
        cont: Box::new(Local::Send { to: role("A"), label: lbl("resp"), cont: Box::new(Local::End) }),
    });
    let g = synthesize(&roles).unwrap();
    let code = codegen_go(&g);
    assert!(code.contains("package main"));
    assert!(code.contains("PROTOCOL OK"));
    assert!(code.contains("DEADLOCK"));
    assert!(code.len() > 500);
}
