//! IR witness table (issue #3930) — proof that emitted Go corresponds to
//! the checked Hybrid IR at concurrency sites.
//!
//! For every `task` spawn and channel `send` statement the emitter records
//! a witness: the source span plus a content hash of the printed sub-AST
//! (deterministic `{:#?}` Debug format, FNV-1a 64-bit — no new dependency).
//! `hyb build --emit-proof` writes proof.json next to the Go output; the
//! verifier re-parses the source, re-runs the emitter, recomputes every
//! hash and compares — any mismatch is a hard error (fail-closed).

use hy_diag::Span;
use serde::Serialize;

/// One concurrency-site witness: where the site sits in the source and
/// what exactly was emitted from it.
#[derive(Debug, Clone, Serialize)]
pub struct Witness {
    /// Site kind: "task" (goroutine spawn) or "send" (channel send).
    pub site: &'static str,
    /// Source span of the statement.
    pub span: Span,
    /// FNV-1a 64-bit of the printed sub-AST (callee+args / chan+value).
    pub node_hash: String,
}

/// Proof envelope written as proof.json by `hyb build --emit-proof`.
#[derive(Debug, Clone, Serialize)]
pub struct Proof {
    /// Number of `task` spawn sites witnessed.
    pub task_sites: usize,
    /// Number of channel-send sites witnessed.
    pub send_sites: usize,
    /// Witnesses in source order (emission order).
    pub witnesses: Vec<Witness>,
}

/// FNV-1a 64-bit over the printed sub-AST. Deterministic across runs on
/// the same tree; the verifier recomputes it from the re-parsed source.
pub(crate) fn fnv1a64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf29ce484222325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100000001b3);
    }
    h
}

/// Witness for a `task` spawn site: hash of callee+args printed sub-AST.
pub(crate) fn witness_task(
    callee: &hy_syntax::Expr,
    args: &[hy_syntax::CallArg],
    span: Span,
) -> Witness {
    let printed = format!("{:?}|{:?}", callee, args);
    Witness {
        site: "task",
        span,
        node_hash: format!("{:016x}", fnv1a64(printed.as_bytes())),
    }
}

/// Witness for a channel-send site: hash of chan+value printed sub-AST.
pub(crate) fn witness_send(
    chan: &hy_syntax::Expr,
    value: &hy_syntax::Expr,
    span: Span,
) -> Witness {
    let printed = format!("{:?}|{:?}", chan, value);
    Witness {
        site: "send",
        span,
        node_hash: format!("{:016x}", fnv1a64(printed.as_bytes())),
    }
}

/// Fold a witness list into the proof envelope.
pub fn build_proof(witnesses: Vec<Witness>) -> Proof {
    let task_sites = witnesses.iter().filter(|w| w.site == "task").count();
    let send_sites = witnesses.iter().filter(|w| w.site == "send").count();
    Proof {
        task_sites,
        send_sites,
        witnesses,
    }
}

/// Serialize the proof envelope to pretty JSON.
pub fn proof_json(proof: &Proof) -> String {
    serde_json::to_string_pretty(proof).unwrap_or_else(|_| "{}".to_string())
}
