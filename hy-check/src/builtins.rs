//! Built‑in checks for the Hybrid compiler.
//! This module contains the logic that validates escape sites, including the
//! newly added Dual‑Polarity Foreign Escape Freeze (O5.3).

use crate::analysis::{self, ShareGroupId, EscapeFreeze};
use crate::diagnostics::{Diagnostic, DiagnosticId};
use crate::hir::*;
use crate::ty::*;

/// Checks a call site that may cause a foreign escape.
///
/// * If the argument is a closure, we inspect its captured variables.
/// * Writes to captured mutable bindings are already rejected by HYB‑E3013.
/// * Reads of captured `may_share` bindings now trigger an *escape‑freeze*.
pub fn check_escape_site(call: &CallExpr, ctx: &mut analysis::Context) {
    // Identify foreign calls (net/http handlers, time.AfterFunc, etc.)
    if !ctx.is_foreign_call(call) {
        return;
    }

    for arg in &call.args {
        if let ExprKind::Closure(ref closure) = arg.kind {
            // Partition captures into reads and writes.
            let (reads, writes) = closure.captures.partition(|c| c.is_read);

            // Existing rule: reject writes to mutable captures.
            for write in writes {
                if write.is_mutable {
                    ctx.emit(Diagnostic::new(
                        DiagnosticId::Hybe3013,
                        format!(
                            "closure writes mutable capture `{}` which escapes to foreign code",
                            write.name
                        ),
                        write.span,
                    ));
                }
            }

            // NEW: handle read‑only captures that belong to a share group.
            for read in reads {
                if read.is_may_share {
                    // Record an escape‑freeze witness for the share group.
                    let sg = ctx.share_group_of(read.binding_id);
                    ctx.record_escape_freeze(sg, call.span);
                }
            }
        }
    }
}

/// Extension trait for the analysis context to support escape‑freeze.
trait EscapeFreezeExt {
    fn record_escape_freeze(&mut self, sg: ShareGroupId, span: Span);
    fn is_frozen(&self, sg: ShareGroupId) -> bool;
}

impl EscapeFreezeExt for analysis::Context {
    fn record_escape_freeze(&mut self, sg: ShareGroupId, span: Span) {
        self.escape_freeze
            .add_frozen_group(sg, span);
    }

    fn is_frozen(&self, sg: ShareGroupId) -> bool {
        self.escape_freeze.is_group_frozen(sg)
    }
}

// The rest of the file (existing checks) remains unchanged.
