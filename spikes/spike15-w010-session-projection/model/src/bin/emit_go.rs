use std::collections::BTreeMap;
use w010_session_projection::*;

fn role(s: &str) -> Role { s.to_string() }
fn lbl(s: &str) -> Label { s.to_string() }

fn main() {
    let args: Vec<String> = std::env::args().collect();
    if args.len() < 2 {
        eprintln!("usage: emit_go <out-main.go>");
        std::process::exit(2);
    }
    let target = &args[1];
    let mut roles: BTreeMap<Role, Local> = BTreeMap::new();
    roles.insert(role("A"), Local::Send {
        to: role("B"), label: lbl("req"),
        cont: Box::new(Local::Recv { from: role("B"), label: lbl("resp"), cont: Box::new(Local::End) }),
    });
    roles.insert(role("B"), Local::Recv {
        from: role("A"), label: lbl("req"),
        cont: Box::new(Local::Send { to: role("A"), label: lbl("resp"), cont: Box::new(Local::End) }),
    });
    let g = synthesize(&roles).expect("request/reply must synthesize");
    let code = codegen_go(&g);
    std::fs::write(target, &code).expect("write output");
    println!("wrote {} bytes to {}", code.len(), target);
}
