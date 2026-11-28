//! W-010 feasibility spike: session-type projection + exhaustive interleaving simulation
//!
//! See ../PREREG.md for the pre-registered hypothesis and kill criteria.

use std::collections::{BTreeMap, BTreeSet};

pub type Role = String;
pub type Label = String;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Global {
    End,
    Msg { from: Role, to: Role, label: Label, cont: Box<Global> },
    Choice { at: Role, to: Role, branches: Vec<(Label, Global)> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Local {
    End,
    Send { to: Role, label: Label, cont: Box<Local> },
    Recv { from: Role, label: Label, cont: Box<Local> },
    Select { to: Role, branches: Vec<(Label, Local)> },
    Branch { from: Role, branches: Vec<(Label, Local)> },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectError {
    ChoiceTargetsSelf { at: Role },
    UninvolvedBranchMerge { role: Role, at: Role },
    EmptyChoice { at: Role },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SynthError {
    NoMatchingReceiver { sender: Role, to: Role, label: Label },
    NoSenderForReceiver { receiver: Role },
    BranchLabelMismatch { sender: Role, to: Role },
    MaxDepth,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimEvent {
    Send { from: Role, to: Role, label: Label },
    Select { from: Role, to: Role, label: Label },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SimResult {
    Terminates { states: usize, trace: Vec<SimEvent> },
    Deadlock { states: usize, witness: Vec<SimEvent>, stuck: Vec<Role> },
    DepthExceeded { states: usize },
}

// ----------------------------- projection -----------------------------

pub fn project(g: &Global, role: &Role) -> Result<Local, ProjectError> {
    match g {
        Global::End => Ok(Local::End),
        Global::Msg { from, to, label, cont } => {
            let c = project(cont, role)?;
            if role == from {
                Ok(Local::Send { to: to.clone(), label: label.clone(), cont: Box::new(c) })
            } else if role == to {
                Ok(Local::Recv { from: from.clone(), label: label.clone(), cont: Box::new(c) })
            } else {
                Ok(c)
            }
        }
        Global::Choice { at, to, branches } => {
            if at == to {
                return Err(ProjectError::ChoiceTargetsSelf { at: at.clone() });
            }
            if branches.is_empty() {
                return Err(ProjectError::EmptyChoice { at: at.clone() });
            }
            if role == at {
                let mut brs = Vec::new();
                for (lbl, g) in branches {
                    brs.push((lbl.clone(), project(g, role)?));
                }
                Ok(Local::Select { to: to.clone(), branches: brs })
            } else if role == to {
                let mut brs = Vec::new();
                for (lbl, g) in branches {
                    brs.push((lbl.clone(), project(g, role)?));
                }
                Ok(Local::Branch { from: at.clone(), branches: brs })
            } else {
                let mut acc: Option<Local> = None;
                for (_, g) in branches {
                    let l = project(g, role)?;
                    match &acc {
                        None => acc = Some(l),
                        Some(prev) if prev != &l => {
                            return Err(ProjectError::UninvolvedBranchMerge {
                                role: role.clone(),
                                at: at.clone(),
                            });
                        }
                        _ => {}
                    }
                }
                acc.ok_or(ProjectError::EmptyChoice { at: at.clone() })
            }
        }
    }
}

// ----------------------------- synthesis -----------------------------

const MAX_DEPTH: usize = 200;

pub fn synthesize(roles: &BTreeMap<Role, Local>) -> Result<Global, SynthError> {
    synthesize_depth(roles, 0)
}

fn synthesize_depth(roles: &BTreeMap<Role, Local>, depth: usize) -> Result<Global, SynthError> {
    if depth > MAX_DEPTH {
        return Err(SynthError::MaxDepth);
    }
    if roles.values().all(|l| matches!(l, Local::End)) {
        return Ok(Global::End);
    }

    let senders: Vec<(Role, Local)> = roles.iter()
        .filter(|(_, l)| matches!(l, Local::Send { .. } | Local::Select { .. }))
        .map(|(r, l)| (r.clone(), l.clone()))
        .collect();
    let receivers: Vec<(Role, Local)> = roles.iter()
        .filter(|(_, l)| matches!(l, Local::Recv { .. } | Local::Branch { .. }))
        .map(|(r, l)| (r.clone(), l.clone()))
        .collect();

    for (s_role, s_loc) in &senders {
        match s_loc {
            Local::Send { to, label, cont } => {
                if let Some(Local::Recv { from: f, label: l, cont: c }) = roles.get(to) {
                    if f == s_role && l == label {
                        let mut new = roles.clone();
                        new.insert(s_role.clone(), (**cont).clone());
                        new.insert(to.clone(), (**c).clone());
                        let rest = synthesize_depth(&new, depth + 1)?;
                        return Ok(Global::Msg {
                            from: s_role.clone(),
                            to: to.clone(),
                            label: label.clone(),
                            cont: Box::new(rest),
                        });
                    }
                }
            }
            Local::Select { to, branches } => {
                if let Some(Local::Branch { from: f, branches: t_branches }) = roles.get(to) {
                    if f == s_role && t_branches.len() == branches.len() {
                        let labels_match = branches.iter().all(|(lbl, _)|
                            t_branches.iter().any(|(tl, _)| tl == lbl));
                        if labels_match {
                            let mut out_branches = Vec::new();
                            for (lbl, s_cont) in branches {
                                let t_cont = t_branches.iter()
                                    .find(|(tl, _)| tl == lbl)
                                    .map(|(_, c)| c.clone())
                                    .unwrap();
                                let mut new = roles.clone();
                                new.insert(s_role.clone(), s_cont.clone());
                                new.insert(to.clone(), t_cont);
                                let sub = synthesize_depth(&new, depth + 1)?;
                                out_branches.push((lbl.clone(), sub));
                            }
                            return Ok(Global::Choice {
                                at: s_role.clone(),
                                to: to.clone(),
                                branches: out_branches,
                            });
                        }
                    }
                }
            }
            _ => {}
        }
    }

    if !senders.is_empty() {
        let (s_role, s_loc) = &senders[0];
        match s_loc {
            Local::Send { to, label, .. } => Err(SynthError::NoMatchingReceiver {
                sender: s_role.clone(), to: to.clone(), label: label.clone(),
            }),
            Local::Select { to, .. } => Err(SynthError::NoMatchingReceiver {
                sender: s_role.clone(), to: to.clone(), label: "<select>".into(),
            }),
            _ => unreachable!(),
        }
    } else if !receivers.is_empty() {
        Err(SynthError::NoSenderForReceiver { receiver: receivers[0].0.clone() })
    } else {
        Ok(Global::End)
    }
}

// ----------------------------- simulation -----------------------------

const MAX_STATES: usize = 20_000;

pub fn simulate_locals(roles: &BTreeMap<Role, Local>) -> SimResult {
    let mut visited: BTreeSet<String> = BTreeSet::new();
    let mut stack: Vec<(BTreeMap<Role, Local>, Vec<SimEvent>)> = vec![(roles.clone(), vec![])];
    let mut states = 0;
    let mut deadlock_witness: Option<(Vec<SimEvent>, Vec<Role>)> = None;
    let mut terminal_trace: Option<Vec<SimEvent>> = None;

    while let Some((state, trace)) = stack.pop() {
        states += 1;
        if states > MAX_STATES {
            return SimResult::DepthExceeded { states };
        }
        let key = format!("{:?}", state);
        if !visited.insert(key) {
            continue;
        }
        if state.values().all(|l| matches!(l, Local::End)) {
            if terminal_trace.is_none() {
                terminal_trace = Some(trace.clone());
            }
            continue;
        }
        let events = enabled_events(&state);
        if events.is_empty() {
            let stuck: Vec<Role> = state.iter()
                .filter(|(_, l)| !matches!(l, Local::End))
                .map(|(r, _)| r.clone())
                .collect();
            if deadlock_witness.is_none() {
                deadlock_witness = Some((trace.clone(), stuck));
            }
            continue;
        }
        for ev in events {
            let next = apply_event(&state, &ev);
            let mut nt = trace.clone();
            nt.push(ev);
            stack.push((next, nt));
        }
    }

    if let Some((witness, stuck)) = deadlock_witness {
        SimResult::Deadlock { states, witness, stuck }
    } else if let Some(trace) = terminal_trace {
        SimResult::Terminates { states, trace }
    } else {
        SimResult::DepthExceeded { states }
    }
}

pub fn enabled_events(state: &BTreeMap<Role, Local>) -> Vec<SimEvent> {
    let mut out = Vec::new();
    for (r, l) in state {
        match l {
            Local::Send { to, label, .. } => {
                if let Some(Local::Recv { from: f, label: l2, .. }) = state.get(to) {
                    if f == r && l2 == label {
                        out.push(SimEvent::Send {
                            from: r.clone(), to: to.clone(), label: label.clone(),
                        });
                    }
                }
            }
            Local::Select { to, branches } => {
                if let Some(Local::Branch { from: f, branches: tb }) = state.get(to) {
                    if f == r {
                        for (lbl, _) in branches {
                            if tb.iter().any(|(tl, _)| tl == lbl) {
                                out.push(SimEvent::Select {
                                    from: r.clone(), to: to.clone(), label: lbl.clone(),
                                });
                            }
                        }
                    }
                }
            }
            _ => {}
        }
    }
    out
}

fn apply_event(state: &BTreeMap<Role, Local>, ev: &SimEvent) -> BTreeMap<Role, Local> {
    let mut out = state.clone();
    match ev {
        SimEvent::Send { from, to, label } => {
            if let Local::Send { cont, .. } = state.get(from).unwrap() {
                out.insert(from.clone(), (**cont).clone());
            }
            if let Local::Recv { cont, .. } = state.get(to).unwrap() {
                out.insert(to.clone(), (**cont).clone());
            }
        }
        SimEvent::Select { from, to, label } => {
            if let Local::Select { branches, .. } = state.get(from).unwrap() {
                let c = branches.iter().find(|(l, _)| l == label).unwrap().1.clone();
                out.insert(from.clone(), c);
            }
            if let Local::Branch { branches, .. } = state.get(to).unwrap() {
                let c = branches.iter().find(|(l, _)| l == label).unwrap().1.clone();
                out.insert(to.clone(), c);
            }
        }
    }
    out
}

// ----------------------------- codegen -----------------------------

fn go_ident(s: &str) -> String {
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if c.is_ascii_alphanumeric() || c == '_' {
            out.push(c);
        } else {
            out.push('_');
        }
        let _ = i;
    }
    if out.is_empty() || out.chars().next().unwrap().is_ascii_digit() {
        out.insert(0, '_');
    }
    out
}

pub fn codegen_go(g: &Global) -> String {
    let mut roles: BTreeSet<Role> = BTreeSet::new();
    collect_roles(g, &mut roles);

    let mut s = String::new();
    s.push_str("// Code generated by w010-spike codegen_go. DO NOT EDIT.\n");
    s.push_str("package main\n\n");
    s.push_str("import (\n\t\"fmt\"\n\t\"sync\"\n\t\"sync/atomic\"\n\t\"time\"\n)\n\n");
    s.push_str("type msg struct{ from, to, label string }\n\n");
    s.push_str("func main() {\n");
    s.push_str("\tvar wg sync.WaitGroup\n");
    s.push_str("\tvar once sync.Once\n");
    for r in &roles {
        s.push_str(&format!("\tinbox_{} := make(chan msg)\n", go_ident(r)));
    }
    s.push_str("\tvar ok int32\n");
    s.push_str("\tdone := make(chan struct{})\n");
    s.push_str("\tfinish := func() { once.Do(func() { close(done) }) }\n");
    s.push_str("\tgo func() {\n");
    s.push_str("\t\t<-time.After(5 * time.Second)\n");
    s.push_str("\t\tif atomic.LoadInt32(&ok) == 0 {\n");
    s.push_str("\t\t\tfmt.Println(\"DEADLOCK\")\n");
    s.push_str("\t\t}\n");
    s.push_str("\t\tfinish()\n");
    s.push_str("\t}()\n");
    for r in &roles {
        let loc = project(g, r).unwrap();
        s.push_str("\twg.Add(1)\n");
        s.push_str("\tgo func() {\n");
        s.push_str("\t\tdefer wg.Done()\n");
        emit_go_local(&loc, &mut s, r, "\t\t");
        s.push_str("\t}()\n");
    }
    s.push_str("\tgo func() {\n");
    s.push_str("\t\twg.Wait()\n");
    s.push_str("\t\tatomic.StoreInt32(&ok, 1)\n");
    s.push_str("\t\tfmt.Println(\"PROTOCOL OK\")\n");
    s.push_str("\t\tfinish()\n");
    s.push_str("\t}()\n");
    s.push_str("\t<-done\n");
    s.push_str("}\n");
    s
}

fn collect_roles(g: &Global, out: &mut BTreeSet<Role>) {
    match g {
        Global::End => {}
        Global::Msg { from, to, cont, .. } => {
            out.insert(from.clone());
            out.insert(to.clone());
            collect_roles(cont, out);
        }
        Global::Choice { at, to, branches } => {
            out.insert(at.clone());
            out.insert(to.clone());
            for (_, b) in branches {
                collect_roles(b, out);
            }
        }
    }
}

fn emit_go_local(l: &Local, s: &mut String, me: &str, indent: &str) {
    match l {
        Local::End => {}
        Local::Send { to, label, cont } => {
            s.push_str(&format!(
                "{}inbox_{} <- msg{{from: {}, to: {}, label: {}}}\n",
                indent, go_ident(to), go_quote(me), go_quote(to), go_quote(label)
            ));
            emit_go_local(cont, s, me, indent);
        }
        Local::Recv { from, label, cont } => {
            s.push_str(&format!(
                "{}<-inbox_{} // recv {} from {}\n",
                indent, go_ident(me), go_quote(label), go_quote(from)
            ));
            emit_go_local(cont, s, me, indent);
        }
        Local::Select { to, branches } => {
            // In a single-sender hub, a select is a deterministic choice; emit
            // the first branch (the model's simulator is the source of truth
            // for choice semantics; this codegen is illustrative).
            if let Some((lbl, c)) = branches.first() {
                s.push_str(&format!(
                    "{}inbox_{} <- msg{{from: {}, to: {}, label: {}}}\n",
                    indent, go_ident(to), go_quote(me), go_quote(to), go_quote(lbl)
                ));
                emit_go_local(c, s, me, indent);
            }
        }
        Local::Branch { from, branches } => {
            if let Some((_, c)) = branches.first() {
                s.push_str(&format!(
                    "{}<-inbox_{} // branch from {}\n",
                    indent, go_ident(me), go_quote(from)
                ));
                emit_go_local(c, s, me, indent);
            }
        }
    }
}

fn go_quote(s: &str) -> String {
    let mut out = String::from("\"");
    for c in s.chars() {
        match c {
            '\\' => out.push_str("\\\\"),
            '"' => out.push_str("\\\""),
            _ => out.push(c),
        }
    }
    out.push('"');
    out
}
