//! Escape‑freeze analysis (O5.3).
//!
//! This module tracks which `share_group`s have been frozen by a foreign
//! callback escape and rejects any later write to a frozen group.

use crate::diagnostics::{Diagnostic, DiagnosticId};
use crate::ty::ShareGroupId;
use std::collections::{HashMap, HashSet};

/// Holds the state for escape‑freeze during a single function analysis.
#[derive(Default)]
pub struct EscapeFreeze {
    /// Map from a share‑group to the span where it was frozen.
    frozen: HashMap<ShareGroupId, Span>,
}

impl EscapeFreeze {
    /// Mark a share group as frozen at the given span.
    pub fn add_frozen_group(&mut self, sg: ShareGroupId, span: Span) {
        self.frozen.entry(sg).or_insert(span);
    }

    /// Query whether a share group is frozen.
    pub fn is_group_frozen(&self, sg: ShareGroupId) -> bool {
        self.frozen.contains_key(&sg)
    }

    /// When a write to a binding occurs, verify it does not belong to a frozen group.
    pub fn check_write(&self, sg: ShareGroupId, write_span: Span, ctx: &mut crate::analysis::Context) {
        if self.is_group_frozen(sg) {
            ctx.emit(Diagnostic::new(
                DiagnosticId::Hybe3014,
                "write to a value that was frozen by a prior foreign‑callback escape".to_string(),
                write_span,
            ));
        }
    }
}

/// Hook called by the statement walker whenever a write to a binding is seen.
pub fn on_write(binding_id: BindingId, span: Span, ctx: &mut crate::analysis::Context) {
    let sg = ctx.share_group_of(binding_id);
    ctx.escape_freeze.check_write(sg, span, ctx);
}

// Integration point: the flat statement/loop walker should call `on_write`
// after it resolves the target binding of an assignment, increment, etc.
