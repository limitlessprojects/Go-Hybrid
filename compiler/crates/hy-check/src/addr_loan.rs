//! addr_loan.rs — T24: no address-of on a loan (HYB-E3104).
//!
//! SPEC-LANGUAGE.md §2.9 (T24) pins a compile-fail claim:
//!
//! ```hy
//! n := &g.nodes[from]      // ok: map-slot loan (O8) — the only general-value `&`
//! p := &n                  // reject: HYB-E3104 — no address-of on a loan
//! ```
//!
//! Before this pass the second line was accepted with 0 diagnostics: the
//! emitter lowered the loan to a copy (`n := g.nodes[from]`) and `&n`
//! became a general Go pointer to that copy — the one value shape T24 says
//! Hybrid does not have, and one whose writes never reach the map slot.
//!
//! A binding is a slot loan when it was defined or re-assigned from
//! `&base[index]` (the same `addr_of_index_base_path` shape that creates
//! the A3 loan). The flag lives on the `Binding`, so a shadowing
//! redefinition in an inner block starts clean (no false rejects).

use crate::model::{Checker, FnState};
use crate::walk::{self, Use};
use hy_diag::{Diagnostic, FixIt};
use hy_syntax::{AssignOp, LocalDecl, Place, PlaceKind};

impl<'a> Checker<'a> {
    /// After `process_local`: flag every name bound to `&m[k]`.
    pub(crate) fn mark_slot_loans_local(&self, st: &mut FnState, l: &LocalDecl) {
        if l.names.len() != l.values.len() {
            return;
        }
        for (name, val) in l.names.iter().zip(l.values.iter()) {
            if walk::addr_of_index_base_path(val).is_some() {
                if let Some(b) = st.env_lookup_mut(name) {
                    b.slot_loan = true;
                }
            }
        }
    }

    /// After `process_assign`: `x = &m[k]` makes `x` a slot loan; any other
    /// plain re-assignment of a whole binding clears the flag.
    pub(crate) fn mark_slot_loans_assign(
        &self,
        st: &mut FnState,
        targets: &[Place],
        op: AssignOp,
        values: &[hy_syntax::Expr],
    ) {
        if op != AssignOp::Assign || targets.len() != values.len() {
            return;
        }
        for (t, v) in targets.iter().zip(values.iter()) {
            if let PlaceKind::Ident(name) = &t.kind {
                let is_loan = walk::addr_of_index_base_path(v).is_some();
                if let Some(b) = st.env_lookup_mut(name) {
                    b.slot_loan = is_loan;
                }
            }
        }
    }

    /// T24: `&x` where `x` is a live slot-loan binding.
    pub(crate) fn check_addr_of_loan(&mut self, st: &FnState, uses: &[Use]) {
        for u in uses.iter().filter(|u| u.via_addr) {
            let Some(b) = self.lookup_binding(st, &u.name) else {
                continue;
            };
            if !b.slot_loan {
                continue;
            }
            let msg = format!(
                "`&{}` takes the address of a loan: a map-slot loan is not an addressable value (T24)",
                u.name
            );
            let mut d = Diagnostic::error("HYB-E3104", self.path.clone(), u.span, msg)
                .with_notes(vec![format!(
                    "`{}` is already the only general-value `&` Hybrid has (an O8 map-slot loan); a pointer to it would be a general pointer, which Tier 0 does not have",
                    u.name
                )]);
            // The registry fix-its for E3104 describe the O2 return-escape
            // shape; replace them with the T24 one for this site.
            d.fixits = vec![FixIt {
                description: format!(
                    "use `{}` directly, or take a fresh loan of the slot (`&m[k]`) where it is needed",
                    u.name
                ),
                replacement: None,
            }];
            self.diags.push(d);
        }
    }
}
