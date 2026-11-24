// hy-check — the Tier-0 checker (SPEC-LANGUAGE.md §9).
//
// Public API (consumed by the `hyb` CLI and the corpus harness):
//   * `check_file(&File) -> CheckResult` — diagnostics + verdict + managed report
//
// Architecture (two passes per function, one flat file pass):
//   * `model.rs`   — the data model: TypeShape classification, bindings,
//     moves/consumes, loans, the Checker accumulator.
//   * `walk.rs`    — Pass A: flattens a body into a source-ordered statement
//     list with indices; records loop/managed extents (the
//     non-lexical windows) and per-binding last-use indices;
//     collects per-statement uses and calls (closures are
//     skipped there — the A5 perimeter; closure BOUNDARY
//     statements are checked by closures.rs).
//   * `closures.rs` — the closure-boundary pass: task/send/select-send
//     statements inside closure bodies are still concurrency
//     boundaries in the emitted Go — checked fail-closed (O4.9),
//     while captures/loans/freeze stay outside the checked
//     subset (A5, documented).
//   * `shape.rs`   — lightweight shape inference from declared types and
//     initializer expressions (NOT a type system).
//   * `foreign.rs`— the builtin foreign-type + method-mode descriptor
//     stubs (documented tech debt: replaced by generated
//     boundary descriptors post-MVP).
//   * `sendability.rs` — A1c (O4 classes) at spawn/send sites + the O6
//     `managed` suppression/lint (HYB-E8001).
//   * `freeze.rs`  — A1b/A7 (O5 freeze-after-move, O13 consume).
//   * `loans.rs`   — A2/A3/A6/LOAN-ESCAPE + the environment updates.
//   * `const_overflow.rs` — HYB-E6002: constant init outside a fixed-width declared type
//     (issue #148; #201-B extends the fold to constant expressions and float→int).
//   * `emit_guard.rs` — HYB-E2006/E1003/E2007/E6005/E6004: expression and
//     declaration guards that keep accepted programs emitting buildable Go
//     (issue #201: incomparable `==`, package-level redeclarations,
//     invalid receivers, negative shift counts, bad array lengths).
//   * this file    — Pass B: the per-statement driver and the verdict.
//   * `elided_literal.rs` — HYB-E1002: a type-elided struct literal `{...}`
//     outside a composite element slot the emitter can fill (Row 13).
//   * `builtins.rs` — HYB-E2001 defer-non-call + HYB-E2005 builtin args (issue #187).
//
// Tier-0 discipline (O4.9, rule FC — human directive 2026-10-13): the
// checker is FAIL-CLOSED at spawn/send boundaries — a value whose type it
// cannot resolve is UNPROVEN, never silently safe (HYB-E3005); the escape
// is the explicit, auditable `managed` hatch (HYB-E8001). Inside a
// function, unknown locals remain legal (no whole-program type system);
// every rejection teaches (O17 — fix-its come from the hy-diag registry);
// diagnostics are returned in source order.

mod addr_loan;
mod addressable;
mod arith;
mod buildable;
mod builtins;
pub mod channel_liveness;
mod closures;
mod const_init;
mod const_overflow;
mod elided_literal;
mod emit_guard;
mod foreign;
mod freeze;
mod go_keyword_names;
mod loans;
mod model;
mod range_iter;
mod recv;
mod resolve;

use hy_ast::{Decl, File, Stmt, Type};
use hy_diag::{Diagnostic, DiagCode};
use model::{CheckResult, Checker, Verdict};

/// Runs Pass B and all sub-pass checks across the parsed file, returning
/// the accumulated diagnostics and overall verdict.
pub fn check_file(file: &File) -> CheckResult {
    let mut checker = Checker::new(file);

    // Reserved prelude helper names check (HYB-E2008 or equivalent emission guard)
    // to prevent user top-level declarations of `hy_pop`, `hy_n5_make`, etc.
    check_prelude_collisions(file, &mut checker);

    // First pass: gather top-level declarations and resolve basic types.
    resolve::resolve_file(file, &mut checker);

    // Second pass: statement-level checks (loans, freeze, sendability, builtins).
    for decl in &file.decls {
        match decl {
            Decl::Fn(f) => {
                checker.enter_fn(f);
                walk::walk_fn(f, &mut checker);
                checker.exit_fn();
            }
            Decl::Struct(s) => {
                emit_guard::check_struct(s, &mut checker);
            }
            Decl::Const(c) => {
                const_init::check_const(c, &mut checker);
            }
            Decl::Global(g) => {
                emit_guard::check_global(g, &mut checker);
            }
        }
    }

    checker.finalize()
}

fn check_prelude_collisions(file: &File, checker: &mut Checker) {
    let forbidden_names = ["hy_pop", "hy_n5_make"];
    for decl in &file.decls {
        let name = match decl {
            Decl::Fn(f) => Some((&f.name, f.span)),
            Decl::Struct(s) => Some((&s.name, s.span)),
            Decl::Const(c) => Some((&c.name, c.span)),
            Decl::Global(g) => Some((&g.name, g.span)),
        };
        if let Some((n, span)) = name {
            if forbidden_names.contains(&n.as_str()) {
                checker.error_at(
                    span,
                    format!(
                        "declaration name `{}` collides with an internal hybrid prelude helper",
                        n
                    ),
                );
            }
        }
    }
}