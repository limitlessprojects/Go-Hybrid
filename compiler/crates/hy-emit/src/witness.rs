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
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Witness {
    /// Site kind: "task" (goroutine spawn) or "send" (channel send).
    pub site: &'static str,
    /// Source span of the statement.
    pub span: Span,
    /// FNV-1a 64-bit of the printed sub-AST (callee+args / chan+value+move).
    pub node_hash: String,
    /// FNV-1a 64-bit of the emitted Go statement at this site.
    pub emitted_hash: String,
}

/// Proof envelope written as proof.json by `hyb build --emit-proof`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Proof {
    /// Number of `task` spawn sites witnessed.
    pub task_sites: usize,
    /// Number of channel-send sites witnessed.
    pub send_sites: usize,
    /// FNV-1a 64-bit of the complete emitted Go source.
    pub emitted_go_hash: String,
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

/// Witness for a `task` spawn site: hash of callee+args printed sub-AST
/// and the emitted Go statement.
pub(crate) fn witness_task(
    callee: &hy_syntax::Expr,
    args: &[hy_syntax::CallArg],
    emitted_go: &str,
    span: Span,
) -> Witness {
    let printed = format!("{:?}|{:?}", callee, args);
    Witness {
        site: "task",
        span,
        node_hash: format!("{:016x}", fnv1a64(printed.as_bytes())),
        emitted_hash: format!("{:016x}", fnv1a64(emitted_go.as_bytes())),
    }
}

/// Witness for a channel-send site: hash of chan+value+move printed sub-AST
/// and the emitted Go statement.
pub(crate) fn witness_send(
    chan: &hy_syntax::Expr,
    value: &hy_syntax::Expr,
    is_move: bool,
    emitted_go: &str,
    span: Span,
) -> Witness {
    let printed = format!("{:?}|{:?}|move={}", chan, value, is_move);
    Witness {
        site: "send",
        span,
        node_hash: format!("{:016x}", fnv1a64(printed.as_bytes())),
        emitted_hash: format!("{:016x}", fnv1a64(emitted_go.as_bytes())),
    }
}

/// Fold a witness list into the proof envelope.
pub fn build_proof(witnesses: Vec<Witness>) -> Proof {
    build_proof_with_go(witnesses, "")
}

/// Fold a witness list and complete emitted Go source into the proof envelope.
pub fn build_proof_with_go(witnesses: Vec<Witness>, emitted_go: &str) -> Proof {
    let task_sites = witnesses.iter().filter(|w| w.site == "task").count();
    let send_sites = witnesses.iter().filter(|w| w.site == "send").count();
    Proof {
        task_sites,
        send_sites,
        emitted_go_hash: format!("{:016x}", fnv1a64(emitted_go.as_bytes())),
        witnesses,
    }
}

/// Serialize the proof envelope to pretty JSON.
pub fn proof_json(proof: &Proof) -> String {
    serde_json::to_string_pretty(proof).unwrap_or_else(|_| "{}".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn emit_go_with_proof_binds_emitted_go_and_site_hashes() {
        let src = "fn worker(n int) {\n    _ = n\n}\n\nfn main() {\n    ch := make(chan int, 1)\n    x := 1\n    ch <- move x\n    v := <-ch\n    task worker(v)\n}\n";
        let ast = hy_syntax::parse_file("t.hy", src).expect("parse failed");
        let (go, proof_str) = crate::emit_go_with_proof(&ast).expect("emit failed");
        let parsed: serde_json::Value =
            serde_json::from_str(&proof_str).expect("valid proof json");

        assert_eq!(parsed["send_sites"], 1);
        assert_eq!(parsed["task_sites"], 1);
        assert_eq!(
            parsed["emitted_go_hash"],
            format!("{:016x}", fnv1a64(go.as_bytes()))
        );

        let witnesses = parsed["witnesses"].as_array().expect("witnesses array");
        assert_eq!(witnesses.len(), 2);
        assert_eq!(witnesses[0]["site"], "send");
        assert_eq!(
            witnesses[0]["emitted_hash"],
            format!("{:016x}", fnv1a64("ch <- x".as_bytes()))
        );
        assert_eq!(witnesses[1]["site"], "task");
        assert_eq!(
            witnesses[1]["emitted_hash"],
            format!("{:016x}", fnv1a64("go worker(v)".as_bytes()))
        );
    }

    #[test]
    fn witness_send_distinguishes_move_from_non_move() {
        let src_move = "fn main() {\n    ch := make(chan int, 1)\n    x := 1\n    ch <- move x\n}\n";
        let src_plain = "fn main() {\n    ch := make(chan int, 1)\n    x := 1\n    ch <- x\n}\n";

        let ast_move = hy_syntax::parse_file("t.hy", src_move).expect("parse move");
        let ast_plain = hy_syntax::parse_file("t.hy", src_plain).expect("parse plain");

        let (_, proof_move_str) = crate::emit_go_with_proof(&ast_move).expect("emit move");
        let (_, proof_plain_str) = crate::emit_go_with_proof(&ast_plain).expect("emit plain");

        let p_move: serde_json::Value = serde_json::from_str(&proof_move_str).unwrap();
        let p_plain: serde_json::Value = serde_json::from_str(&proof_plain_str).unwrap();

        assert_eq!(
            p_move["witnesses"][0]["emitted_hash"],
            p_plain["witnesses"][0]["emitted_hash"],
            "Go emission drops move marker so emitted statement matches"
        );
        assert_ne!(
            p_move["witnesses"][0]["node_hash"],
            p_plain["witnesses"][0]["node_hash"],
            "node_hash must bind Stmt::Send.is_move"
        );
    }

    #[test]
    fn proof_emitted_go_hash_detects_non_concurrency_edits() {
        let src_a = "fn main() {\n    x := 1\n    _ = x\n}\n";
        let src_b = "fn main() {\n    x := 2\n    _ = x\n}\n";

        let ast_a = hy_syntax::parse_file("t.hy", src_a).unwrap();
        let ast_b = hy_syntax::parse_file("t.hy", src_b).unwrap();

        let (_, proof_a_str) = crate::emit_go_with_proof(&ast_a).unwrap();
        let (_, proof_b_str) = crate::emit_go_with_proof(&ast_b).unwrap();

        assert_ne!(proof_a_str, proof_b_str);
        let pa: serde_json::Value = serde_json::from_str(&proof_a_str).unwrap();
        let pb: serde_json::Value = serde_json::from_str(&proof_b_str).unwrap();
        assert_ne!(pa["emitted_go_hash"], pb["emitted_go_hash"]);
    }
}
