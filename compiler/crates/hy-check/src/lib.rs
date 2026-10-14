//! hy-check — the Tier-0 checker (SPEC-LANGUAGE.md §9).
//!
//! Public API (consumed by the `hyb` CLI and the corpus harness):
//!   * `check_file(&File) -> CheckResult` — diagnostics + verdict + managed report
//!
//! Architecture (two passes per function, one flat file pass):
//!   * `model.rs`   — the data model: TypeShape classification, bindings,
//!     moves/consumes, loans, the Checker accumulator.
//!   * `walk.rs`    — Pass A: flattens a body into a source-ordered statement
//!     list with indices; records loop/managed extents (the
//!     non-lexical windows) and per-binding last-use indices;
//!     collects per-statement uses and calls (closures are
//!     skipped there — the A5 perimeter; closure BOUNDARY
//!     statements are checked by closures.rs).
//!   * `closures.rs` — the closure-boundary pass: task/send/select-send
//!     statements inside closure bodies are still concurrency
//!     boundaries in the emitted Go — checked fail-closed (O4.9),
//!     while captures/loans/freeze stay outside the checked
//!     subset (A5, documented).
//!   * `shape.rs`   — lightweight shape inference from declared types and
//!     initializer expressions (NOT a type system).
//!   * `foreign.rs`— the builtin foreign-type + method-mode descriptor
//!     stubs (documented tech debt: replaced by generated
//!     boundary descriptors post-MVP).
//!   * `sendability.rs` — A1c (O4 classes) at spawn/send sites + the O6
//!     `managed` suppression/lint (HYB-E8001).
//!   * `freeze.rs`  — A1b/A7 (O5 freeze-after-move, O13 consume).
//!   * `loans.rs`   — A2/A3/A6/LOAN-ESCAPE + the environment updates.
//!   * this file    — Pass B: the per-statement driver and the verdict.
//!
//! Tier-0 discipline (O4.9, rule FC — human directive 2026-10-13): the
//! checker is FAIL-CLOSED at spawn/send boundaries — a value whose type it
//! cannot resolve is UNPROVEN, never silently safe (HYB-E3005); the escape
//! is the explicit, auditable `managed` hatch (HYB-E8001). Inside a
//! function, unknown locals remain legal (no whole-program type system);
//! every rejection teaches (O17 — fix-its come from the hy-diag registry);
//! diagnostics are returned in source order.

mod closures;
mod foreign;
mod freeze;
mod loans;
mod model;
mod sendability;
mod shape;
mod walk;

use hy_diag::{Diagnostic, ManagedBlockAudit, ManagedReport, Severity};

use hy_syntax::{Decl, Expr, File, FnDecl, ForKind, SelectArmKind, Stmt};
use model::{Binding, Checker, FnState, MANAGED_CATEGORIES, TypeShape};

/// The result of checking one file.
#[derive(Debug, Default)]
pub struct CheckResult {
    pub diagnostics: Vec<Diagnostic>,
    /// `accept` when no errors remain (warnings allowed), else `reject`.
    pub verdict: Verdict,
    pub managed: ManagedReport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Verdict {
    #[default]
    Accept,
    Reject,
}

impl Verdict {
    pub fn as_str(&self) -> &'static str {
        match self {
            Verdict::Accept => "accept",
            Verdict::Reject => "reject",
        }
    }
}

/// Check one parsed file against the Tier-0 rules (O2/O4/O5/O6/O7/O8/O12...).
///
/// Verdict = reject iff any severity-error diagnostic survives (warnings —
/// including the managed lint — never fail a build). Diagnostics are sorted
/// into source order before returning: the corpus harness reads the FIRST
/// error's `tier0_rule` as the join key against VERDICTS.md.
pub fn check_file(file: &File) -> CheckResult {
    let mut ck = Checker::new(file);

    build_tables(&mut ck);
    ck.check_struct_decls(); // A6: borrow-handle / pointer struct fields

    let fns: Vec<&FnDecl> = file.fns().collect();
    for f in fns {
        check_fn(&mut ck, f);
    }

    let mut diags = ck.diags;
    diags.sort_by(|a, b| {
        (
            a.span.start.line,
            a.span.start.col,
            a.span.end.line,
            a.span.end.col,
        )
            .cmp(&(
                b.span.start.line,
                b.span.start.col,
                b.span.end.line,
                b.span.end.col,
            ))
    });
    let verdict = if diags.iter().any(|d| d.severity == Severity::Error) {
        Verdict::Reject
    } else {
        Verdict::Accept
    };
    CheckResult {
        diagnostics: diags,
        verdict,
        managed: ManagedReport {
            managed_blocks: ck.managed_blocks,
            sites_via_managed: ck.sites_via_managed,
            total_sites: ck.total_sites,
            categories: {
                let mut m = std::collections::BTreeMap::new();
                for b in &ck.managed_audit {
                    *m.entry(b.category.clone()).or_insert(0usize) += 1;
                }
                m
            },
            blocks: ck.managed_audit.clone(),
        },
    }
}

// ---------------------------------------------------------------------------
// File-level tables
// ---------------------------------------------------------------------------

fn build_tables(ck: &mut Checker<'_>) {
    let file: &File = ck.file;
    for d in &file.decls {
        match d {
            Decl::Import { path, .. } => {
                // `import "rust/serde_json"` → package root "serde_json".
                let root = path.rsplit('/').next().unwrap_or(path.as_str());
                ck.packages.insert(root.to_string());
            }
            Decl::Const(c) => {
                let b = ck.const_binding(c);
                ck.consts.insert(c.name.clone(), b);
            }
            Decl::Struct(s) => {
                // `type X = int` aliases parse as zero-field struct decls;
                // they are not containers — leave them unresolved (Unknown
                // is permissive, and alias-of-primitive values must never
                // classify as mutable GC structs).
                if s.fields.is_empty() {
                    continue;
                }
                ck.structs.insert(
                    s.name.clone(),
                    s.fields
                        .iter()
                        .map(|f| (f.name.clone(), f.ty.clone()))
                        .collect(),
                );
            }
            Decl::Fn(f) => {
                ck.fns.insert(f.name.clone(), f);
            }
            Decl::Enum(_) => {}
            Decl::TypeAlias(_) => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Pass B — the per-function statement driver
// ---------------------------------------------------------------------------

fn check_fn(ck: &mut Checker<'_>, f: &FnDecl) {
    let Some(body) = &f.body else { return };

    // environment seed: receiver + params
    let mut st = FnState::default();
    if let Some(r) = &f.receiver {
        let shape = ck.shape_of_type(&r.ty);
        st.env.insert(
            r.name.clone(),
            Binding {
                shape,
                ty: Some(r.ty.clone()),
                immutable: false,
                managed_taint: false,
                moved: None,
            },
        );
    }
    for p in &f.params {
        let shape = ck.shape_of_type(&p.ty);
        st.env.insert(
            p.name.clone(),
            Binding {
                shape,
                ty: Some(p.ty.clone()),
                immutable: false,
                managed_taint: false,
                moved: None,
            },
        );
    }

    // Pass A: indices, structural extents, last-use map.
    let flat = walk::flatten(body);
    st.last_use = flat.last_use.clone();

    // Pass B: statements in source order.
    for (stmt, idx) in &flat.stmts {
        let stmt: &Stmt = stmt;
        let idx = *idx;
        // Issue #14: apply the statement's scope events BEFORE processing.
        // A Push opens the block scope its defines live in; a Pop closes the
        // scope of a construct that ended at the previous statement, which
        // restores the outer binding (an inner `m := 1` can no longer
        // overwrite the outer map's shape and launder it past a spawn).
        let (push, pop) = flat.events.get(&idx).copied().unwrap_or((false, false));
        if pop {
            st.scope_pop();
        }
        if push {
            st.scope_push();
        }
        let managed = flat.in_managed(idx);
        if let Stmt::Managed {
            reason,
            category,
            tickets,
            span,
            ..
        } = stmt
        {
            ck.managed_blocks += 1; // O6.3: one per `managed` statement
            // Row 14 (discussion #4): the payload is AUDITED here — the
            // parser enforces the literal-list SHAPE, the checker enforces
            // the policy (fail-closed: a bare block cannot pass silently).
            if reason.trim().is_empty() {
                ck.diags.push(
                    Diagnostic::error(
                        "HYB-E0003",
                        ck.path.clone(),
                        *span,
                        "managed block has no reason — the escape hatch must say what it bypasses and why (HYB-E0003)",
                    )
                    .with_notes(vec![
                        "a bare `managed { ... }` is a reject, not a lint — the hatch must be review-visible (discussion #4)"
                            .to_string(),
                    ]),
                );
            }
            if category.is_empty() {
                ck.diags.push(
                    Diagnostic::error(
                        "HYB-E0003",
                        ck.path.clone(),
                        *span,
                        "managed block has no category — name the invariant being bypassed (HYB-E0003)",
                    )

                );
            } else if !MANAGED_CATEGORIES.contains(&category.as_str()) {
                ck.diags.push(
                    Diagnostic::error(
                        "HYB-E0004",
                        ck.path.clone(),
                        *span,
                        format!("unknown managed category `\"{}\"` (HYB-E0004)", category),
                    )
                    .with_notes(vec![
                        format!(
                            "valid categories: {} — the category tells reviewers WHICH invariant the block bypasses; `other` requires the reason to carry the specifics",
                            MANAGED_CATEGORIES.join(", ")
                        ),
                    ]),
                );
            }
            ck.managed_audit.push(ManagedBlockAudit {
                reason: reason.clone(),
                category: category.clone(),
                tickets: tickets.clone(),
            });
        }

        // A1b/A7: use-after-move on this statement's own uses (checked
        // BEFORE this statement marks any new transfers).
        let mut uses = Vec::new();
        walk::own_uses(stmt, &mut uses);
        ck.check_uses(&st, &uses);

        // Calls (A2 loan windows, A7 consumes + arg lending). A for-loop
        // ITERATE expression's borrow calls get loop-extent loans: the
        // element views live across the whole loop body (O7).
        if let Stmt::For {
            kind: ForKind::Iterate { expr, .. },
            ..
        } = stmt
        {
            let loop_end = flat.loop_end_of(idx);
            let mut calls = Vec::new();
            walk::expr_calls(expr, &mut calls);
            for cr in calls {
                ck.process_call(&mut st, &cr, idx, Some(loop_end));
            }
        } else {
            let mut calls = Vec::new();
            walk::own_calls(stmt, &mut calls);
            for cr in calls {
                ck.process_call(&mut st, &cr, idx, None);
            }
        }

        // Statement-specific processing.
        match stmt {
            Stmt::Local(l) => ck.process_local(&mut st, l, idx, managed),

            Stmt::Assign {
                targets,
                op,
                values,
                span,
            } => {
                ck.process_assign(&mut st, targets, *op, values, *span, idx);
            }

            Stmt::Return { values, .. } => ck.process_return(&mut st, values),

            // O4.0 check-site: every task argument (HYB-E3001).
            Stmt::Task { callee, args, span } => {
                let desc = format!("the spawn `task {}(...)`", callee.render());
                let vals: Vec<&Expr> = args.iter().map(|a| &a.value).collect();
                ck.check_site(&mut st, *span, "HYB-E3001", &vals, managed, &desc, false);
            }

            // O4.0 check-site: the sent value (HYB-E3002). The parser flags
            // `ch <- move x` as `is_move: true` with a plain Ident value —
            // `implicit_move` routes it to the class-3 move handling.
            Stmt::Send {
                chan,
                value,
                is_move,
                span,
            } => {
                let desc = if *is_move {
                    format!(
                        "the move-send `{} <- move {}`",
                        chan.render(),
                        value.render()
                    )
                } else {
                    format!("the send `{} <- {}`", chan.render(), value.render())
                };
                let vals: Vec<&Expr> = vec![value];
                ck.check_site(&mut st, *span, "HYB-E3002", &vals, managed, &desc, *is_move);
            }

            // Loop variables: fresh bindings per iteration shape.
            Stmt::For { kind, .. } => match kind {
                ForKind::Iterate { vars, expr } => {
                    let shapes = ck.iterate_var_shapes(&st, expr, vars.len());
                    for (name, shape) in vars.iter().zip(shapes) {
                        ck.define_binding(&mut st, name, shape, None, false, managed);
                    }
                }
                ForKind::Range { var, .. } => {
                    ck.define_binding(&mut st, var, TypeShape::Primitive, None, false, managed);
                }
                ForKind::While { .. } | ForKind::Infinite => {}
            },

            // select arms: `ch <- v` is a send check-site; `v, ok := <-ch`
            // defines its names (Unknown — fail-closed if they later cross
            // a boundary, O4.9: ascribe or route through a known chan).
            Stmt::Select { arms, .. } => {
                for arm in arms {
                    match &arm.kind {
                        SelectArmKind::Send { chan, value } => {
                            let desc = format!(
                                "the select-send `{} <- {}`",
                                chan.render(),
                                value.render()
                            );
                            let span = chan.span().to(value.span());
                            let vals: Vec<&Expr> = vec![value];
                            ck.check_site(&mut st, span, "HYB-E3002", &vals, managed, &desc, false);
                        }
                        SelectArmKind::Recv { names, chan } => {
                            // `v[, ok] := <-ch` — the value's shape is the
                            // channel's ELEMENT when the channel resolves
                            // (fewer unknowns); `ok` is the receive flag
                            // (a bool). An unresolvable element stays
                            // Unknown — fail-closed if v later crosses a
                            // boundary (O4.9).
                            let elem = match ck.shape_of_expr(&st, chan) {
                                TypeShape::Chan(e) => *e,
                                _ => TypeShape::Unknown,
                            };
                            for (i, n) in names.iter().enumerate() {
                                let shape = if i == 0 {
                                    elem.clone()
                                } else {
                                    TypeShape::Primitive
                                };
                                ck.define_binding(&mut st, n, shape, None, false, managed);
                            }
                        }
                        SelectArmKind::Default => {}
                    }
                }
            }

            // If-init Locals, match arms, defer, exprs, blocks: already
            // covered by the flat walk (init is its own statement entry)
            // and the uses/calls collected above.
            _ => {}
        }
    }

    // All remaining block scopes die at the end of the function (issue #14):
    // the closure pass's capture view is the COMPLETED base env (A5) — a
    // capture of a block-local now resolves to nothing and fails closed
    // (HYB-E3005) at the boundary, exactly like any unresolvable name.
    st.scopes.clear();

    // Closure-boundary pass (closures.rs): task/send/select-send STATEMENTS
    // inside closure bodies are concurrency boundaries in the emitted Go —
    // checked here, fail-closed (O4.9), with the completed function env as
    // the capture view. Captures/loans/freeze stay outside (A5, documented).
    closures::check_closure_boundaries(ck, &st, body);
}

// ---------------------------------------------------------------------------
// Tests — parsed-snippet checks per rule family (verdict + first-error rule)
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use hy_syntax::parse_file;

    fn check(src: &str) -> CheckResult {
        let ast = parse_file("t.hy", src).expect("snippet parses");
        check_file(&ast)
    }

    fn first_error_rule(r: &CheckResult) -> Option<String> {
        r.diagnostics
            .iter()
            .find(|d| d.severity == Severity::Error)
            .and_then(|d| d.tier0_rule.clone())
    }

    #[test]
    fn clean_program_accepts() {
        let r = check("fn main() { x := 1 \n fmt.println(x) }");
        assert_eq!(r.verdict, Verdict::Accept);
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
    }

    #[test]
    fn a1c_spawn_shared_map_rejects() {
        let r = check(
            "fn tally(counts map[string]int) { counts[\"a\"]++ }\n\
             fn main() {\n\
               counts := map[string]int{}\n\
               task tally(counts)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1c"));
        let d = &r.diagnostics[0];
        assert_eq!(d.code, "HYB-E3001");
        assert!(d.message.contains("counts"));
        assert!(d.message.contains("map[string]int"));
        assert!(!d.fixits.is_empty(), "O17: rejection must teach");
    }

    #[test]
    fn a1c_send_shared_struct_rejects() {
        let r = check(
            "struct s { n int }\n\
             fn main() {\n\
               st := s{}\n\
               ch := make(chan s, 1)\n\
               ch <- st\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1c"));
        assert_eq!(r.diagnostics[0].code, "HYB-E3002");
    }

    #[test]
    fn a1c_addr_of_struct_rejects() {
        let r = check(
            "struct st { n int }\n\
             fn w(s *st) { s.n++ }\n\
             fn main() {\n\
               stats := st{}\n\
               task w(&stats)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1c"));
        assert!(r.diagnostics[0].message.contains("&stats"));
        assert!(r.diagnostics[0].message.contains("*st"));
    }

    #[test]
    fn a1c_fresh_literal_and_move_and_chan_ok() {
        let r = check(
            "struct s { a int }\n\
             fn w(x s, c chan<- s) { c <- s{a: 1} }\n\
             fn main() {\n\
               v := s{a: 2}\n\
               c := make(chan s, 1)\n\
               task w(move v, c)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
        assert!(r.diagnostics.is_empty());
    }

    #[test]
    fn a1b_write_after_move_send() {
        let r = check(
            "fn main() {\n\
               agg := map[string]int{}\n\
               agg[\"w\"]++\n\
               results := make(chan map[string]int, 1)\n\
               results <- move agg\n\
               agg[\"total\"] = len(agg)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1b"));
        let d = &r.diagnostics[0];
        assert_eq!(d.code, "HYB-E3003");
        assert!(d.message.contains("agg"));
    }

    #[test]
    fn a1b_read_after_move_send() {
        let r = check(
            "fn main() {\n\
               agg := map[string]int{}\n\
               results := make(chan map[string]int, 1)\n\
               results <- move agg\n\
               n := len(agg)\n\
               fmt.println(n)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1b"));
        assert_eq!(r.diagnostics[0].code, "HYB-E3003");
    }

    #[test]
    fn a1b_alias_write_after_move() {
        let r = check(
            "fn main() {\n\
               agg := map[string]int{}\n\
               m2 := agg\n\
               results := make(chan map[string]int, 1)\n\
               results <- move agg\n\
               m2[\"total\"] = len(m2)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1b"));
        let d = &r.diagnostics[0];
        assert!(d.message.contains("alias"), "{}", d.message);
    }

    #[test]
    fn a2_mutate_during_loan_rejects() {
        let r = check(
            "import \"rust/serde_json\"\n\
             fn main() {\n\
               doc := serde_json.parse(`{}`)?\n\
               for u in doc.array(\"users\")? {\n\
                 if u.int(\"logins\")? == 0 {\n\
                   doc.set(\"pruned\", true)?\n\
                 }\n\
               }\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A2"));
        let d = &r.diagnostics[0];
        assert_eq!(d.code, "HYB-E3101");
        assert!(d.message.contains("doc"));
    }

    #[test]
    fn a2_mutate_after_loan_dies_accepts() {
        let r = check(
            "import \"rust/serde_json\"\n\
             fn main() {\n\
               doc := serde_json.parse(`{}`)?\n\
               for u in doc.array(\"users\")? {\n\
                 fmt.println(u.int(\"logins\")?)\n\
               }\n\
               doc.set(\"pruned\", true)?\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    }

    #[test]
    fn a3_insert_inside_window_rejects() {
        let r = check(
            "struct svc { n int }\n\
             struct env { m map[string]svc }\n\
             fn f(e *env) {\n\
               s := &e.m[\"a\"]\n\
               e.m[\"b\"] = svc{n: 1}\n\
               s.n = 2\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A3"));
        let d = &r.diagnostics[0];
        assert_eq!(d.code, "HYB-E3102");
        assert!(d.message.contains("insert"));
    }

    #[test]
    fn a3_insert_after_last_use_accepts() {
        let r = check(
            "struct svc { n int }\n\
             struct env { m map[string]svc }\n\
             fn f(e *env) {\n\
               s := &e.m[\"a\"]\n\
               s.n = 2\n\
               e.m[\"b\"] = svc{n: 1}\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    }

    #[test]
    fn a6_borrow_handle_field_rejects() {
        let r = check(
            "import \"rust/serde_json\"\n\
             struct auditor {\n\
               trail []string\n\
               cur   serde_json.view\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A6"));
        let d = &r.diagnostics[0];
        assert_eq!(d.code, "HYB-E3103");
        assert!(d.message.contains("auditor.cur"));
        assert!(d.message.contains("serde_json.view"));
    }

    #[test]
    fn loan_escape_return_addr_of_rejects() {
        let r = check(
            "struct c { hits int }\n\
             struct reg { cs []c }\n\
             fn get(r *reg, i int) *c {\n\
               return &r.cs[i]\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject);
        assert_eq!(first_error_rule(&r).as_deref(), Some("LOAN-ESCAPE"));
        let d = &r.diagnostics[0];
        assert_eq!(d.code, "HYB-E3104");
        assert!(d.message.contains("borrow"));
    }

    #[test]
    fn a7_use_after_consuming_rejects() {
        let r = check(
            "import \"rust/flate2\"\n\
             fn main() {\n\
               enc := flate2.new_writer(6)?\n\
               out := enc.finish()?\n\
               enc.write([]byte(\"trailer\"))?\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A7"));
        let d = &r.diagnostics[0];
        assert_eq!(d.code, "HYB-E3003");
        assert!(d.message.contains("enc"));
    }

    #[test]
    fn a7_borrow_by_default_and_return_transfer_accept() {
        // 12-shape: two borrow calls then a mutate — windows non-lexical.
        let r = check(
            "import \"rust/serde_json\"\n\
             fn count(doc serde_json.value) int {\n\
               n := 0\n\
               for u in doc.array(\"users\")? {\n\
                 n += u.int(\"logins\")?\n\
               }\n\
               return n\n\
             }\n\
             fn main() {\n\
               doc := serde_json.parse(`{}`)?\n\
               a := count(doc)?\n\
               b := count(doc)?\n\
               doc.set(\"audited\", true)?\n\
               fmt.println(a + b)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    }

    #[test]
    fn managed_suppression_lints_and_accepts() {
        let r = check(
            "fn tally(counts map[string]int) { counts[\"a\"]++ }\n\
             fn main() {\n\
               managed(\"counters merged under the test's own mutex\", \"mutex\", \"PROJ-42\") {\n\
                 counts := map[string]int{}\n\
                 task tally(counts)\n\
               }\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E8001"),
            "managed lint must fire: {:?}",
            r.diagnostics
        );
        assert!(r.diagnostics.iter().all(|d| d.severity != Severity::Error));
        assert_eq!(r.managed.managed_blocks, 1);
        assert!(r.managed.sites_via_managed >= 1);
        assert!(r.managed.total_sites >= 1);
        // row 14: the audit is a table, not a bare number
        assert_eq!(r.managed.categories.get("mutex"), Some(&1));
        assert_eq!(r.managed.blocks.len(), 1);
        assert_eq!(r.managed.blocks[0].tickets, vec!["PROJ-42".to_string()]);
        assert!(!r.managed.blocks[0].reason.is_empty());
    }

    #[test]
    fn managed_bare_block_rejects_e0003() {
        // row 14 fail-closed: no payload = coded reject
        let r = check(
            "fn tally(counts map[string]int) { counts[\"a\"]++ }\n\
             fn main() {\n\
               managed {\n\
                 counts := map[string]int{}\n\
                 task tally(counts)\n\
               }\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        let codes: Vec<&str> = r.diagnostics.iter().map(|d| d.code.as_str()).collect();
        assert!(codes.contains(&"HYB-E0003"), "{:?}", r.diagnostics);
        // BOTH missing parts are named (reason + category)
        assert_eq!(
            r.diagnostics
                .iter()
                .filter(|d| d.code == "HYB-E0003")
                .count(),
            2
        );
    }

    #[test]
    fn managed_unknown_category_rejects_e0004() {
        let r = check(
            "fn tally(counts map[string]int) { counts[\"a\"]++ }\n\
             fn main() {\n\
               managed(\"r\", \"misc\") {\n\
                 counts := map[string]int{}\n\
                 task tally(counts)\n\
               }\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E0004"),
            "{:?}",
            r.diagnostics
        );
        let e4 = r
            .diagnostics
            .iter()
            .find(|d| d.code == "HYB-E0004")
            .unwrap();
        assert!(e4.notes[0].contains("mutex"), "fix-it lists the set");
    }

    #[test]
    fn atomic_and_const_immutable_spawn_args_accept() {
        // 17-shape: `&hits` where hits is atomic-typed; const table read.
        let r = check(
            "import \"sync/atomic\"\n\
             const th = [10, 50, 250, 1000]\n\
             struct sample { probe int }\n\
             fn probe(id int, hits *atomic.int64, s chan<- sample) {\n\
               if th[3] > 100 {\n\
                 hits.add(1)\n\
               }\n\
               s <- sample{probe: id}\n\
             }\n\
             fn main() {\n\
               var hits atomic.int64\n\
               samples := make(chan sample, 4)\n\
               task probe(0, &hits, samples)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    }

    #[test]
    fn a1c_const_mutable_gc_value_is_not_class4_immutable() {
        // T22: `const` is the ONLY deep immutability in MVP, and it is pinned
        // as `[TBD]` for slices/maps ("faking it would create a false safety
        // claim"). A `const`-bound map has a mutable interior, so the class-4
        // shortcut must not apply: the value is a named mutable GC value and
        // the spawn is HYB-E3001 (A1c).
        let r = check(
            "const shared = map[string]int{\"k\": 0}\n\
             fn worker(m map[string]int) {\n\
               m[\"k\"] = 1\n\
             }\n\
             fn main() {\n\
               task worker(shared)\n\
               shared[\"k\"] = 2\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1c"));
        assert_eq!(r.diagnostics[0].code, "HYB-E3001");
    }

    #[test]
    fn a1c_fn_local_const_map_spawn_rejects() {
        // Same gap via a fn-local `const` (LocalKind::Const) — the binding
        // shortcut and the file-level const table must agree.
        let r = check(
            "fn worker(m map[string]int) {\n\
               m[\"k\"] = 1\n\
             }\n\
             fn main() {\n\
               const shared = map[string]int{\"k\": 0}\n\
               task worker(shared)\n\
               shared[\"k\"] = 2\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1c"));
    }

    #[test]
    fn a1c_const_slice_channel_send_rejects() {
        // The send check-site (HYB-E3002) takes the same class-4 path as the
        // spawn site: a `const` slice crossing a channel send is not sendable.
        let r = check(
            "const xs = []int{0, 0}\n\
             fn main() {\n\
               ch := make(chan []int, 1)\n\
               ch <- xs\n\
               xs[0] = 2\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(r.diagnostics[0].code, "HYB-E3002");
    }

    #[test]
    fn a1c_let_bound_mutable_gc_value_is_not_class4_immutable() {
        // T22: `let` is shallow — interior mutation through an alias is still
        // possible, so a `let`-bound map is a named mutable GC value.
        let r = check(
            "fn worker(m map[string]int) {\n\
               m[\"k\"] = 1\n\
             }\n\
             fn main() {\n\
               let shared = map[string]int{\"k\": 0}\n\
               task worker(shared)\n\
               shared[\"k\"] = 2\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1c"));
    }

    #[test]
    fn a1c_let_bound_field_view_is_not_class4_immutable() {
        // The Field/Index/Slice arm took its own `immutable` shortcut: a view
        // reached through a `let`/`const` root must be classified by the
        // view's own shape, not by the root's binding-level immutability.
        let r = check(
            "struct holder {\n\
               data map[string]int\n\
             }\n\
             fn worker(m map[string]int) {\n\
               m[\"k\"] = 1\n\
             }\n\
             fn main() {\n\
               let h = holder{data: map[string]int{\"k\": 0}}\n\
               task worker(h.data)\n\
               h.data[\"k\"] = 2\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1c"));
    }

    #[test]
    fn a1c_immutable_by_type_constants_still_accept() {
        // Positive control (guard against over-strictness): `const` values
        // that ARE immutable-by-type must keep crossing boundaries freely —
        // a string and a primitive (O4 class 4), plus an index READ out of a
        // const array literal, which is the shape the existing
        // `atomic_and_const_immutable_spawn_args_accept` test pins.
        let r = check(
            "const greeting = \"hello\"\n\
             const limit = 42\n\
             const table = [10, 50, 250]\n\
             fn worker(s string, n int, first int) {\n\
               fmt.println(s)\n\
               fmt.println(n)\n\
               fmt.println(first)\n\
             }\n\
             fn main() {\n\
               task worker(greeting, limit, table[0])\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    }

    #[test]
    fn a1c_const_array_literal_binding_is_not_sendable() {
        // A `const` bound to an array literal is inferred as a Slice shape by
        // today's lightweight inference (`[10, 50]` -> []primitive), and a
        // slice aliases its backing storage — so the binding is NOT
        // immutable-by-type and must not cross a boundary. Pinned so the
        // positive control above cannot be widened silently.
        let r = check(
            "const table = [10, 50, 250]\n\
             fn worker(t []int) {\n\
               fmt.println(t[0])\n\
             }\n\
             fn main() {\n\
               task worker(table)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1c"));
    }

    #[test]
    fn a2_define_stored_view_extends_loan() {
        // `u := doc.array(...)` stores an element view: the borrow loan on
        // the handle lives until the view's last use (O7 non-lexical) — a
        // mutating call inside that window conflicts.
        let r = check(
            "import \"rust/serde_json\"\n\
             fn main() {\n\
               doc := serde_json.parse(`{}`)?\n\
               u := doc.array(\"users\")?\n\
               doc.set(\"x\", true)?\n\
               fmt.println(u.int(\"n\")?)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A2"));
        assert_eq!(r.diagnostics[0].code, "HYB-E3101");
    }

    #[test]
    fn a2_primitive_boundary_read_is_a_copy() {
        // O9: a boundary read of a primitive is a COPY — it must NOT keep
        // the handle borrowed; the later mutating call is legal.
        let r = check(
            "import \"rust/serde_json\"\n\
             fn main() {\n\
               doc := serde_json.parse(`{}`)?\n\
               n := doc.int(\"n\")?\n\
               doc.set(\"x\", true)?\n\
               fmt.println(n)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    }

    #[test]
    fn a7_own_transfer_at_return_accepts() {
        // 13-shape: `return v` transfers the own handle OUT; the caller
        // mutates the returned handle — the transfer made it the caller's.
        let r = check(
            "import \"rust/serde_json\"\n\
             fn summary(doc serde_json.value) serde_json.value {\n\
               v := serde_json.new_object()\n\
               for u in doc.array(\"users\")? {\n\
                 if u.int(\"logins\")? > 0 {\n\
                   v.set(\"active\", 1)\n\
                 }\n\
               }\n\
               return v\n\
             }\n\
             fn main() {\n\
               doc := serde_json.parse(`{}`)?\n\
               s := summary(doc)?\n\
               s.set(\"cached_at\", \"2026-10-12\")?\n\
               fmt.println(serde_json.to_string_pretty(s)?)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
        assert!(r.diagnostics.is_empty());
    }

    #[test]
    fn o46_deep_sendability_of_composites() {
        // A fresh literal is NOT automatically safe: embedding a live
        // mutable map by name is rejected, naming that part (O4.6).
        let r = check(
            "struct job { data map[string]int }\n\
             fn w(j job) { }\n\
             fn main() {\n\
               shared_map := map[string]int{}\n\
               ch := make(chan job, 1)\n\
               ch <- job{data: shared_map}\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1c"));
        let d = &r.diagnostics[0];
        assert_eq!(d.code, "HYB-E3002");
        assert!(d.message.contains("shared_map"), "{}", d.message);
    }

    #[test]
    fn fresh_literal_with_sendable_parts_accepts() {
        // The clean twin: every reference-typed part sendable.
        let r = check(
            "struct probe { id int }\n\
             struct sample { p probe }\n\
             fn w(s chan<- sample) { }\n\
             fn main() {\n\
               id := 3\n\
               c := make(chan sample, 1)\n\
               task w(c)\n\
               c <- sample{p: probe{id: id}}\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    }

    // -------------------------------------------------------------------
    // O4.9 fail-closed boundary (rule FC, HYB-E3005) — human directive
    // 2026-10-13: what the checker cannot prove, it does not call safe.
    // The escape is the explicit, auditable `managed` hatch (HYB-E8001).
    // -------------------------------------------------------------------

    #[test]
    fn fc_unknown_initializer_at_boundary_rejects() {
        // the initializer's type does not resolve (unstubbed foreign call) →
        // the binding is Unknown → crossing is E3005, not a silent pass
        let r = check(
            "fn handle(x string) { fmt.println(x) }\n\
             fn main() {\n\
               blob := strings.repeat(\"a\", 3)\n\
               task handle(blob)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        let d = r
            .diagnostics
            .iter()
            .find(|d| d.code == "HYB-E3005")
            .expect("E3005 must fire for the unproven value");
        assert_eq!(d.tier0_rule.as_deref(), Some("FC"));
        assert!(d.message.contains("blob"), "{}", d.message);
        assert!(
            !d.fixits.is_empty(),
            "O17: the diagnostic must teach the fix"
        );
    }

    #[test]
    fn fc_unresolved_name_at_boundary_rejects() {
        // a name that does not resolve cannot be proven safe — it used to
        // pass silently (Finding 1, sendability.rs)
        let r = check(
            "fn handle(x int) { fmt.println(x) }\n\
             fn main() {\n\
               task handle(nosuch)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E3005"));
    }

    #[test]
    fn fc_e3005_fixit_suggests_current_managed_syntax() {
        // Issue #7 (bounty wording sweep): the HYB-E3005 fix-it used to tell
        // readers to write a bare `managed { ... }` — syntax the checker now
        // rejects outright (HYB-E0003: reason + category are required, see
        // fc_unresolved_name_at_boundary_rejects' sibling test below). The
        // fix-it must name the hatch that actually compiles.
        let r = check(
            "fn handle(x int) { fmt.println(x) }\n\
             fn main() {\n\
               task handle(nosuch)\n\
             }",
        );
        let e3005 = r
            .diagnostics
            .iter()
            .find(|d| d.code == "HYB-E3005")
            .expect("unresolved name at a spawn boundary is HYB-E3005");
        assert!(
            e3005
                .notes
                .iter()
                .any(|n| n.contains("managed(\"why this bypass is safe\", \"category\")")),
            "fix-it must suggest the real managed(reason, category) syntax: {:?}",
            e3005.notes
        );
        assert!(
            !e3005.notes.iter().any(|n| n.contains("managed { ")),
            "fix-it must not suggest the bare `managed {{ ... }}` form, which HYB-E0003 rejects: {:?}",
            e3005.notes
        );
    }

    #[test]
    fn fc_unproven_name_via_managed_escapes() {
        // the SAME unproven value inside a managed block escapes through the
        // explicit hatch: warning only, counted — never a silent pass
        let r = check(
            "fn handle(x int) { fmt.println(x) }\n\
             fn main() {\n\
               managed(\"test-only handler; name shape pinned by the fixture\", \"other\") {\n\
                 task handle(nosuch)\n\
               }\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
        assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E8001"));
        assert!(r.diagnostics.iter().all(|d| d.severity != Severity::Error));
        assert_eq!(r.managed.managed_blocks, 1);
        assert_eq!(r.managed.sites_via_managed, 1);
    }

    #[test]
    fn fc_ascription_makes_the_share_provable() {
        // the fix-it from the E3005 diagnostic: ascribe the type at the
        // declaration — the checker can now classify (string = class 4)
        let r = check(
            "fn handle(x string) { fmt.println(x) }\n\
             fn main() {\n\
               var blob string = strings.repeat(\"a\", 3)\n\
               task handle(blob)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
        assert!(r.diagnostics.is_empty());
    }

    #[test]
    fn fc_select_recv_known_element_classifies() {
        // `v, ok := <-ch` — the value's shape is the channel's ELEMENT when
        // the channel resolves (no manufactured unknown)
        let r = check(
            "fn take(x int) { fmt.println(x) }\n\
             fn main() {\n\
               ch := make(chan int, 1)\n\
               select {\n\
               v, ok := <-ch: { task take(v) }\n\
               }\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    }

    #[test]
    fn fc_select_recv_unknown_element_rejects_at_boundary() {
        // the channel's element did not resolve → v is Unknown → v crossing
        // is E3005 (this surface used to define a permissive Unknown)
        let r = check(
            "fn take(x int) { fmt.println(x) }\n\
             fn main() {\n\
               ch := make(chan mystery, 1)\n\
               select {\n\
               v, ok := <-ch: { task take(v) }\n\
               }\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E3005"));
    }

    #[test]
    fn fc_closure_boundary_send_of_shared_map_rejects() {
        // Finding 2: a send INSIDE a closure body is a real boundary in the
        // emitted Go — the closures.rs pass owns it now
        let r = check(
            "fn main() {\n\
               counts := map[string]int{}\n\
               ch := make(chan int, 1)\n\
               mux := http.new_serve_mux()\n\
               mux.handle(\"/x\", http.handler(fn(w http.response_writer) {\n\
                 ch <- counts\n\
               }))\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        let d = r
            .diagnostics
            .iter()
            .find(|d| d.code == "HYB-E3002")
            .expect("the closure-body send must be checked");
        assert!(d.message.contains("counts"), "{}", d.message);
    }

    #[test]
    fn fc_closure_boundary_unproven_capture_rejects() {
        // fail-closed inside closures too: an unresolved capture at a
        // closure-body boundary is E3005, not a silent pass
        let r = check(
            "fn main() {\n\
               ch := make(chan int, 1)\n\
               mux := http.new_serve_mux()\n\
               mux.handle(\"/x\", http.handler(fn(w http.response_writer) {\n\
                 ch <- ghost\n\
               }))\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E3005"));
    }

    #[test]
    fn fc_closure_boundary_send_inside_managed_escapes() {
        // the hatch works inside closures (managed_blocks parity: the
        // closure pass counts its managed blocks for --report-managed)
        let r = check(
            "fn main() {\n\
               counts := map[string]int{}\n\
               ch := make(chan int, 1)\n\
               mux := http.new_serve_mux()\n\
               mux.handle(\"/x\", http.handler(fn(w http.response_writer) {\n\
                 managed { ch <- counts }\n\
               }))\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
        assert!(r.diagnostics.iter().any(|d| d.code == "HYB-E8001"));
        assert_eq!(r.managed.managed_blocks, 1);
        assert_eq!(r.managed.sites_via_managed, 1);
    }

    #[test]
    fn fc_closure_without_boundary_stays_outside_perimeter() {
        // corpus 10 shape unchanged: a capture used INSIDE a closure without
        // any task/send statement is still the A5 perimeter (Q1, OQ1) — the
        // closure pass owns boundary statements, nothing more
        let r = check(
            "fn main() {\n\
               hits := 0\n\
               mux := http.new_serve_mux()\n\
               mux.handle(\"/ping\", http.handler(fn(w http.response_writer) {\n\
                 hits++\n\
               }))\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
    }

    #[test]
    fn closure_capture_is_outside_perimeter() {
        // 10-shape: FnLit bodies are never analyzed (A5 scoped claim).
        let r = check(
            "fn main() {\n\
               hits := 0\n\
               mux := http.new_serve_mux()\n\
               mux.handle(\"/ping\", http.handler(fn(w http.response_writer) {\n\
                 hits++\n\
               }))\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    }

    #[test]
    fn diagnostics_are_sorted_in_source_order() {
        let r = check(
            "fn tally(counts map[string]int) { counts[\"a\"]++ }\n\
             fn main() {\n\
               counts := map[string]int{}\n\
               task tally(counts)\n\
               results := make(chan map[string]int, 1)\n\
               results <- move counts\n\
               n := len(counts)\n\
               fmt.println(n)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject);
        let spans: Vec<_> = r
            .diagnostics
            .iter()
            .map(|d| (d.span.start.line, d.span.start.col))
            .collect();
        let mut sorted = spans.clone();
        sorted.sort();
        assert_eq!(spans, sorted);
        // first error in source order is the spawn (A1c)
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1c"));
    }

    #[test]
    fn o54_move_on_gc_field_at_spawn_rejects_e3004() {
        let r = check(
            "struct holder { data map[string]int }\n\
             fn worker(m map[string]int) { m[\"k\"] = 1 }\n\
             fn main() {\n\
               h := holder{data: map[string]int{\"k\": 0}}\n\
               task worker(move h.data)\n\
               h.data[\"k\"] = 2\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1b"));
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
            "{:?}",
            r.diagnostics
        );
    }

    #[test]
    fn o54_move_on_gc_field_at_send_rejects_e3004() {
        let r = check(
            "struct holder { data map[string]int }\n\
             fn main() {\n\
               s := holder{data: map[string]int{}}\n\
               jobs := make(chan map[string]int, 1)\n\
               jobs <- move s.data\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1b"));
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
            "{:?}",
            r.diagnostics
        );
    }

    #[test]
    fn o54_move_on_gc_field_at_local_define_rejects_e3004() {
        let r = check(
            "struct holder { data map[string]int }\n\
             fn main() {\n\
               s := holder{data: map[string]int{}}\n\
               m := move s.data\n\
               fmt.println(m)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert!(
            r.diagnostics.iter().any(|d| d.code == "HYB-E3004"),
            "{:?}",
            r.diagnostics
        );
    }

    #[test]
    fn o51_parenthesized_and_composite_nested_move_freezes_binding() {
        let r1 = check(
            "fn worker(m map[string]int) { m[\"k\"] = 1 }\n\
             fn main() {\n\
               counts := map[string]int{}\n\
               task worker(move (counts))\n\
               counts[\"k\"] = 2\n\
             }",
        );
        assert_eq!(r1.verdict, Verdict::Reject, "{:?}", r1.diagnostics);
        assert!(r1.diagnostics.iter().any(|d| d.code == "HYB-E3003"));

        let r2 = check(
            "struct job { data map[string]int }\n\
             fn main() {\n\
               counts := map[string]int{}\n\
               ch := make(chan job, 1)\n\
               ch <- job{data: move counts}\n\
               counts[\"k\"] = 2\n\
             }",
        );
        assert_eq!(r2.verdict, Verdict::Reject, "{:?}", r2.diagnostics);
        assert!(r2.diagnostics.iter().any(|d| d.code == "HYB-E3003"));
    }

    // -----------------------------------------------------------------------
    // Scope-aware bindings (issue #14, LubuSeb's soundness report): an
    // inner-block define must shadow, never overwrite. The flat env let
    // `m := 1` inside an `if` rewrite the outer map's binding, and the
    // later `task worker(m)` classified against the INNER int and passed
    // with zero diagnostics while the emitted Go raced (go -race: two
    // write/write DATA RACEs).
    // -----------------------------------------------------------------------

    #[test]
    fn shadow_scope_does_not_launder_outer_map_e3001() {
        // The exact issue-#14 repro: the inner `m := 1` shadows the outer
        // map; the spawn AFTER the block must still see the MAP.
        let r = check(
            "fn worker(m map[string]int, done chan<- bool) {\n\
               for i in 0..10000 { m[\"k\"] = i }\n\
               done <- true\n\
             }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               if true {\n\
                 m := 1\n\
                 fmt.println(m)\n\
               }\n\
               done := make(chan bool, 1)\n\
               task worker(m, done)\n\
               for i in 0..10000 { m[\"k\"] = i }\n\
               <-done\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1c"));
        assert_eq!(r.diagnostics[0].code, "HYB-E3001");
    }

    #[test]
    fn shadow_inner_primitive_is_sendable_inside_block() {
        // The inner binding itself is a plain int: a boundary INSIDE the
        // block classifies against it and must stay accepted (no
        // over-rejection of legal shadowing).
        let r = check(
            "fn take(i int) { fmt.println(i) }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               if true {\n\
                 m := 1\n\
                 task take(m)\n\
               }\n\
               fmt.println(len(m))\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
    }

    #[test]
    fn shadow_does_not_leak_across_sibling_blocks() {
        // Two sibling ifs each define their own m; the second block must
        // not inherit the first's binding, and the outer map after both
        // must still reject at the spawn.
        let r = check(
            "fn worker(m map[string]int) { m[\"k\"] = 1 }\n\
             fn take(i int) { fmt.println(i) }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               if true {\n\
                 m := 1\n\
                 task take(m)\n\
               }\n\
               if true {\n\
                 m := 2\n\
                 task take(m)\n\
               }\n\
               task worker(m)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1c"));
    }

    #[test]
    fn if_init_define_does_not_overwrite_outer_binding() {
        // An if-init define shares Go's if-scope; after the if, the outer
        // binding is intact and the spawn must classify against the MAP.
        let r = check(
            "fn worker(m map[string]int) { m[\"k\"] = 1 }\n\
             fn main() {\n\
               m := map[string]int{\"k\": 0}\n\
               if x := 1; x > 0 {\n\
                 fmt.println(x)\n\
               }\n\
               task worker(m)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1c"));
    }

    #[test]
    fn let_in_closure_pass_is_shallow_too() {
        // The closure-boundary pass must agree with the flat pass on T22
        // (issue #12 parity): a let-bound map captured... here, a let-bound
        // map defined in the closure body itself is NOT class-4.
        let r = check(
            "fn main() {\n\
               fn() {\n\
                 let m = map[string]int{\"k\": 0}\n\
                 task bump(m)\n\
               }()\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
    }

    // -----------------------------------------------------------------------
    // `new(T)` returns `*T`, not a primitive (O4 / O4.9). Regression for the
    // accepted-but-unsound shape `p := new(S); task w(p)`, which shared a
    // `*S` across tasks with zero diagnostics and raced under `go -race`.
    // -----------------------------------------------------------------------

    #[test]
    fn new_builtin_spawn_pointer_rejects() {
        let r = check(
            "struct pool_stats { processed int }\n\
             fn bump(s *pool_stats) { s.processed = s.processed + 1 }\n\
             fn main() {\n\
               p := new(pool_stats)\n\
               task bump(p)\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1c"));
        assert_eq!(r.diagnostics[0].code, "HYB-E3001");
        assert!(r.diagnostics[0].message.contains("*pool_stats"));
    }

    #[test]
    fn new_builtin_send_pointer_rejects() {
        let r = check(
            "fn main() {\n\
               jobs := make(chan int, 1)\n\
               p := new(int)\n\
               jobs <- p\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Reject, "{:?}", r.diagnostics);
        assert_eq!(first_error_rule(&r).as_deref(), Some("A1c"));
        assert_eq!(r.diagnostics[0].code, "HYB-E3002");
    }

    #[test]
    fn new_builtin_local_only_use_accepts() {
        // No boundary: a locally-confined `new` must not be rejected.
        let r = check(
            "struct pool_stats { processed int }\n\
             fn main() {\n\
               p := new(pool_stats)\n\
               p.processed = 5\n\
               fmt.println(\"{p.processed}\")\n\
             }",
        );
        assert_eq!(r.verdict, Verdict::Accept, "{:?}", r.diagnostics);
        assert!(r.diagnostics.is_empty(), "{:?}", r.diagnostics);
    }
}

