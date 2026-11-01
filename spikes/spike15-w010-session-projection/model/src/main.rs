use std::collections::BTreeMap;
use w010_session_projection::*;

fn role(s: &str) -> Role { s.to_string() }
fn lbl(s: &str) -> Label { s.to_string() }

fn main() {
    println!("=== W-010 session-type projection spike ===\n");

    // ---- Scenario 1: A/B ping-pong (the canonical deadlock) ----
    println!("[1] A/B ping-pong: A sends p then recvs q; B sends q then recvs p");
    let mut roles: BTreeMap<Role, Local> = BTreeMap::new();
    roles.insert(role("A"), Local::Send {
        to: role("B"), label: lbl("p"),
        cont: Box::new(Local::Recv { from: role("B"), label: lbl("q"), cont: Box::new(Local::End) }),
    });
    roles.insert(role("B"), Local::Send {
        to: role("A"), label: lbl("q"),
        cont: Box::new(Local::Recv { from: role("A"), label: lbl("p"), cont: Box::new(Local::End) }),
    });

    match synthesize(&roles) {
        Ok(g) => println!("  UNEXPECTED: synthesized {:?}\n", g),
        Err(e) => println!("  REJECTED as prereg predicted: {:?}\n", e),
    }

    // Simulate the same locals directly (no global to project from)
    let sim = simulate_locals(&roles);
    match &sim {
        SimResult::Deadlock { states, witness, stuck } => {
            println!("  simulation: DEADLOCK after {} states; witness={:?}; stuck={:?}\n",
                states, witness, stuck);
        }
        SimResult::Terminates { states, trace } => {
            println!("  simulation: UNEXPECTED TERMINATES in {} states trace={:?}\n", states, trace);
        }
        SimResult::DepthExceeded { states } => {
            println!("  simulation: depth exceeded after {} states\n", states);
        }
    }

    // ---- Scenario 2: request/reply ----
    println!("[2] Request/reply: A sends req; B recvs req then sends resp; A recvs resp");
    let mut roles2: BTreeMap<Role, Local> = BTreeMap::new();
    roles2.insert(role("A"), Local::Send {
        to: role("B"), label: lbl("req"),
        cont: Box::new(Local::Recv { from: role("B"), label: lbl("resp"), cont: Box::new(Local::End) }),
    });
    roles2.insert(role("B"), Local::Recv {
        from: role("A"), label: lbl("req"),
        cont: Box::new(Local::Send { to: role("A"), label: lbl("resp"), cont: Box::new(Local::End) }),
    });

    let g2 = match synthesize(&roles2) {
        Ok(g) => g,
        Err(e) => { println!("  UNEXPECTED synth err: {:?}\n", e); return; }
    };
    println!("  synthesized global: {:?}", g2);

    // Round-trip projection
    let pa = project(&g2, &role("A")).unwrap();
    let pb = project(&g2, &role("B")).unwrap();
    println!("  project(A) == original A: {}", pa == roles2[&role("A")]);
    println!("  project(B) == original B: {}", pb == roles2[&role("B")]);

    // Exhaustive simulation
    match simulate_locals(&roles2) {
        SimResult::Terminates { states, trace } => {
            println!("  simulation: TERMINATES, {} states, trace={:?}\n", states, trace);
        }
        SimResult::Deadlock { states, witness, stuck } => {
            println!("  simulation: UNEXPECTED DEADLOCK, {} states, witness={:?}, stuck={:?}\n",
                states, witness, stuck);
        }
        SimResult::DepthExceeded { states } => {
            println!("  simulation: DEPTH_EXCEEDED after {} states\n", states);
        }
    }

    // ---- Scenario 3: three-role pipeline ----
    println!("[3] Three-role pipeline: A -m1-> B -m2-> C -m3-> A");
    let mut roles3: BTreeMap<Role, Local> = BTreeMap::new();
    roles3.insert(role("A"), Local::Send {
        to: role("B"), label: lbl("m1"),
        cont: Box::new(Local::Recv { from: role("C"), label: lbl("m3"), cont: Box::new(Local::End) }),
    });
    roles3.insert(role("B"), Local::Recv {
        from: role("A"), label: lbl("m1"),
        cont: Box::new(Local::Send { to: role("C"), label: lbl("m2"), cont: Box::new(Local::End) }),
    });
    roles3.insert(role("C"), Local::Recv {
        from: role("B"), label: lbl("m2"),
        cont: Box::new(Local::Send { to: role("A"), label: lbl("m3"), cont: Box::new(Local::End) }),
    });
    match synthesize(&roles3) {
        Ok(g) => {
            println!("  synthesized: {:?}", g);
            match simulate_locals(&roles3) {
                SimResult::Terminates { states, .. } => println!("  simulation: TERMINATES, {} states\n", states),
                SimResult::Deadlock { states, witness, .. } => println!("  simulation: DEADLOCK, {} states, witness={:?}\n", states, witness),
                SimResult::DepthExceeded { states } => println!("  simulation: DEPTH_EXCEEDED {}\n", states),
            }
        }
        Err(e) => println!("  synth err: {:?}\n", e),
    }

    // ---- Scenario 4: three-role cross-cycle (deadlocked) ----
    println!("[4] Three-role cross-cycle: A sends a; B sends b; C sends c (all recv in wrong order)");
    let mut roles4: BTreeMap<Role, Local> = BTreeMap::new();
    roles4.insert(role("A"), Local::Send {
        to: role("B"), label: lbl("a"),
        cont: Box::new(Local::Recv { from: role("C"), label: lbl("c"), cont: Box::new(Local::End) }),
    });
    roles4.insert(role("B"), Local::Send {
        to: role("C"), label: lbl("b"),
        cont: Box::new(Local::Recv { from: role("A"), label: lbl("a"), cont: Box::new(Local::End) }),
    });
    roles4.insert(role("C"), Local::Send {
        to: role("A"), label: lbl("c"),
        cont: Box::new(Local::Recv { from: role("B"), label: lbl("b"), cont: Box::new(Local::End) }),
    });
    match synthesize(&roles4) {
        Ok(g) => println!("  UNEXPECTED synth ok: {:?}", g),
        Err(e) => println!("  synthesis REJECTED: {:?}", e),
    }
    match simulate_locals(&roles4) {
        SimResult::Terminates { states, .. } => println!("  UNEXPECTED terminates in {}\n", states),
        SimResult::Deadlock { states, witness, stuck } => println!("  simulation: DEADLOCK after {} states; witness={:?}; stuck={:?}\n", states, witness, stuck),
        SimResult::DepthExceeded { states } => println!("  depth exceeded {}\n", states),
    }

    // ---- Scenario 5: codegen for request/reply ----
    println!("[5] codegen for request/reply:");
    let code = codegen_go(&g2);
    println!("  generated {} bytes of Go", code.len());

    println!("\n=== done ===");
}
