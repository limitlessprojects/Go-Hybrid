//! Standalone static call-graph stack-bound verifier and Go-ABI0 `NOSPLIT`
//! lowering model for Wildcard #375 (`spike19-certified-nosplit-leaves`).
//!
//! Run verification suite:
//!   rustc --edition=2021 --test spikes/spike19-certified-nosplit-leaves/stack_cert_verifier.rs -o /tmp/spike19_test && /tmp/spike19_test

use std::collections::{BTreeMap, BTreeSet};

/// Maximum cumulative stack frame (in bytes) permitted for a leaf emitted
/// directly as a Go `NOSPLIT` assembly routine (`TEXT ·Foo(SB), NOSPLIT, $frame-args`).
/// Go's linker enforces `StackNosplit` (~792 B total chain); 512 B reserves >= 280 B
/// for caller NOSPLIT frames.
pub const GO_NOSPLIT_CERT_LIMIT: u32 = 512;

/// Guard thresholds from `interfaces/descriptors.md`.
pub const SMALL_GUARD_LIMIT: u32 = 1024;
pub const DEFAULT_GUARD_LIMIT: u32 = 4096;

/// Manual classification currently recorded in `interfaces/descriptors.md`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ManualLedgerClass {
    Small,
    Default,
    Deep,
    Unlisted,
}

/// Machine-checked classification produced by the static call-graph + frame verifier.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CertifiedClass {
    /// Zero-recursion, zero-indirect, zero-external-OS-call leaf with total stack <= 512 B.
    /// Eligible for direct `NOSPLIT` Go `.s` emission without runtime `g.stackguard0` probe.
    NosplitDirect { max_stack_bytes: u32 },
    /// Acyclic, bounded happy-path stack <= 1024 B (`Cross*S` guard 1024 B).
    SmallGuarded {
        max_stack_bytes: u32,
        nosplit_blocker: &'static str,
    },
    /// Acyclic, bounded happy-path stack <= 4096 B (`Cross*` guard 4096 B).
    DefaultGuarded {
        max_stack_bytes: u32,
        small_blocker: &'static str,
    },
    /// Recursive, indirect-call, or > 4096 B frame (`CrossDeep*` 1 MiB pooled stack required).
    DeepPooled { reason: String },
}

#[derive(Debug, Clone)]
pub struct FnNode {
    pub symbol: &'static str,
    pub self_frame_bytes: u32,
    pub callees: Vec<&'static str>,
    pub has_indirect_call: bool,
    pub touches_os_lock_or_alloc: bool,
    pub has_slot_drop_glue_cycle: bool,
    pub manual_class: ManualLedgerClass,
}

#[derive(Debug, Clone)]
pub struct CallGraph {
    pub nodes: BTreeMap<&'static str, FnNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VerificationReport {
    pub symbol: &'static str,
    pub manual_class: ManualLedgerClass,
    pub certified_class: CertifiedClass,
    pub happy_path_stack_bytes: Option<u32>,
    pub ledger_hazard_detected: bool,
}

impl CallGraph {
    pub fn new() -> Self {
        Self {
            nodes: BTreeMap::new(),
        }
    }

    pub fn insert(&mut self, node: FnNode) {
        self.nodes.insert(node.symbol, node);
    }

    /// Computes exact worst-case stack depth along the call graph starting at `root`.
    /// Returns `Err(reason)` if a direct/mutual recursion cycle or indirect call is reachable.
    pub fn analyze_stack(
        &self,
        root: &'static str,
    ) -> Result<(u32, bool, bool), String> {
        let mut visiting = BTreeSet::new();
        let mut memo: BTreeMap<&'static str, (u32, bool, bool)> = BTreeMap::new();
        self.dfs(root, &mut visiting, &mut memo)
    }

    fn dfs(
        &self,
        sym: &'static str,
        visiting: &mut BTreeSet<&'static str>,
        memo: &mut BTreeMap<&'static str, (u32, bool, bool)>,
    ) -> Result<(u32, bool, bool), String> {
        if visiting.contains(sym) {
            return Err(format!("recursive call cycle detected at `{}`", sym));
        }
        if let Some(&cached) = memo.get(sym) {
            return Ok(cached);
        }
        let node = self
            .nodes
            .get(sym)
            .ok_or_else(|| format!("unresolved external symbol `{}`", sym))?;

        if node.has_indirect_call {
            return Err(format!(
                "indirect register call (`blr`/`call *reg`) in `{}`",
                sym
            ));
        }

        visiting.insert(sym);
        let mut max_child_stack = 0u32;
        let mut any_os_or_alloc = node.touches_os_lock_or_alloc;
        let mut any_drop_glue = node.has_slot_drop_glue_cycle;

        for &callee in &node.callees {
            let (child_stack, child_os, child_drop) = self.dfs(callee, visiting, memo)?;
            if child_stack > max_child_stack {
                max_child_stack = child_stack;
            }
            any_os_or_alloc |= child_os;
            any_drop_glue |= child_drop;
        }
        visiting.remove(sym);

        let total = node
            .self_frame_bytes
            .checked_add(max_child_stack)
            .ok_or_else(|| format!("frame size overflow in `{}`", sym))?;
        let res = (total, any_os_or_alloc, any_drop_glue);
        memo.insert(sym, res);
        Ok(res)
    }

    pub fn verify_export(&self, sym: &'static str) -> VerificationReport {
        let node = &self.nodes[sym];
        match self.analyze_stack(sym) {
            Err(reason) => {
                let hazard = matches!(
                    node.manual_class,
                    ManualLedgerClass::Small | ManualLedgerClass::Default
                );
                VerificationReport {
                    symbol: sym,
                    manual_class: node.manual_class,
                    certified_class: CertifiedClass::DeepPooled { reason },
                    happy_path_stack_bytes: None,
                    ledger_hazard_detected: hazard,
                }
            }
            Ok((total_bytes, touches_os, has_drop_glue)) => {
                let certified = if total_bytes > DEFAULT_GUARD_LIMIT {
                    CertifiedClass::DeepPooled {
                        reason: format!(
                            "acyclic stack {} B exceeds DEFAULT guard ({} B)",
                            total_bytes, DEFAULT_GUARD_LIMIT
                        ),
                    }
                } else if total_bytes > SMALL_GUARD_LIMIT {
                    CertifiedClass::DefaultGuarded {
                        max_stack_bytes: total_bytes,
                        small_blocker: "cumulative frame exceeds 1024 B SMALL guard",
                    }
                } else if touches_os {
                    CertifiedClass::SmallGuarded {
                        max_stack_bytes: total_bytes,
                        nosplit_blocker: if has_drop_glue {
                            "calls pthread_mutex_lock/__rust_alloc and links serde_json::Value drop glue in table_insert"
                        } else {
                            "calls opaque libc pthread_mutex_lock/__rust_alloc"
                        },
                    }
                } else if total_bytes > GO_NOSPLIT_CERT_LIMIT {
                    CertifiedClass::SmallGuarded {
                        max_stack_bytes: total_bytes,
                        nosplit_blocker: "pure compute leaf (0 OS calls) but frame > 512 B Go StackNosplit budget",
                    }
                } else {
                    CertifiedClass::NosplitDirect {
                        max_stack_bytes: total_bytes,
                    }
                };

                let hazard = match (node.manual_class, &certified) {
                    (ManualLedgerClass::Small, CertifiedClass::DefaultGuarded { .. })
                    | (ManualLedgerClass::Small, CertifiedClass::DeepPooled { .. })
                    | (ManualLedgerClass::Default, CertifiedClass::DeepPooled { .. }) => true,
                    _ => false,
                };

                VerificationReport {
                    symbol: sym,
                    manual_class: node.manual_class,
                    certified_class: certified,
                    happy_path_stack_bytes: Some(total_bytes),
                    ledger_hazard_detected: hazard,
                }
            }
        }
    }
}

/// Emits a Go Plan 9 assembly stub (`NOSPLIT`) for a certified leaf, or returns an
/// admission error if the export is not `CertifiedClass::NosplitDirect`.
pub fn emit_certified_go_asm_stub(
    report: &VerificationReport,
    go_symbol: &str,
    arg_frame_bytes: u32,
) -> Result<String, String> {
    match report.certified_class {
        CertifiedClass::NosplitDirect { max_stack_bytes } => Ok(format!(
            "// Certified by spike19 static verifier: `{}` max_stack = {} B <= {} B (0 recursion, 0 OS/alloc calls)\n\
             TEXT ·{}(SB), NOSPLIT, ${}-{}\n\
             \t// Direct in-place execution on caller goroutine stack\n\
             \tMOVQ\ta1+0(FP), AX\n\
             \tMOVQ\tAX, ret+8(FP)\n\
             \tRET\n",
            report.symbol, max_stack_bytes, GO_NOSPLIT_CERT_LIMIT, go_symbol, max_stack_bytes, arg_frame_bytes
        )),
        ref other => Err(format!(
            "cannot emit NOSPLIT direct assembly for `{}`: classified as {:?}",
            report.symbol, other
        )),
    }
}

/// Builds the call-graph fixture matching the actual `objdump -dr libhyrt.a` measurements
/// from `runtime/rust/hyrt` (release profile, `panic = "abort"`).
pub fn build_measured_hyrt_graph() -> CallGraph {
    let mut g = CallGraph::new();

    // Shared internal helpers measured in libhyrt.a:
    g.insert(FnNode {
        symbol: "keccak::keccak_p",
        self_frame_bytes: 192, // 0xc0
        callees: vec![],
        has_indirect_call: false,
        touches_os_lock_or_alloc: false,
        has_slot_drop_glue_cycle: false,
        manual_class: ManualLedgerClass::Unlisted,
    });
    g.insert(FnNode {
        symbol: "sha3::Sha3_256Core::update",
        self_frame_bytes: 48, // 0x30
        callees: vec!["keccak::keccak_p"],
        has_indirect_call: false,
        touches_os_lock_or_alloc: false,
        has_slot_drop_glue_cycle: false,
        manual_class: ManualLedgerClass::Unlisted,
    });
    g.insert(FnNode {
        symbol: "serde_json::Value::index_into",
        self_frame_bytes: 96, // 0x60
        callees: vec![],
        has_indirect_call: false,
        touches_os_lock_or_alloc: false,
        has_slot_drop_glue_cycle: false,
        manual_class: ManualLedgerClass::Unlisted,
    });
    g.insert(FnNode {
        symbol: "serde_json::recursive_tree_walk",
        self_frame_bytes: 240,
        callees: vec!["serde_json::recursive_tree_walk"], // self-recursive up to depth 128
        has_indirect_call: true,
        touches_os_lock_or_alloc: true,
        has_slot_drop_glue_cycle: true,
        manual_class: ManualLedgerClass::Unlisted,
    });
    g.insert(FnNode {
        symbol: "regex_automata::compile_and_search",
        self_frame_bytes: 15922, // measured acyclic path inside regex + regex_syntax
        callees: vec![],
        has_indirect_call: true,
        touches_os_lock_or_alloc: true,
        has_slot_drop_glue_cycle: false,
        manual_class: ManualLedgerClass::Unlisted,
    });

    // 1. Pure compute / ABI leaves:
    g.insert(FnNode {
        symbol: "hy_nop",
        self_frame_bytes: 0,
        callees: vec![],
        has_indirect_call: false,
        touches_os_lock_or_alloc: false,
        has_slot_drop_glue_cycle: false,
        manual_class: ManualLedgerClass::Default,
    });
    g.insert(FnNode {
        symbol: "hyrt_abi_word",
        self_frame_bytes: 0,
        callees: vec![],
        has_indirect_call: false,
        touches_os_lock_or_alloc: false,
        has_slot_drop_glue_cycle: false,
        manual_class: ManualLedgerClass::Small,
    });
    g.insert(FnNode {
        symbol: "hyrt_stack_alignment_probe",
        self_frame_bytes: 32,
        callees: vec![],
        has_indirect_call: false,
        touches_os_lock_or_alloc: false,
        has_slot_drop_glue_cycle: false,
        manual_class: ManualLedgerClass::Small,
    });

    // 2. sha3binding exports:
    // Note: hy_sha3_digest256 self=800B + update=48B + keccak_p=192B = 1040B (> 1024B SMALL guard!)
    g.insert(FnNode {
        symbol: "hy_sha3_digest256",
        self_frame_bytes: 800,
        callees: vec!["sha3::Sha3_256Core::update"],
        has_indirect_call: false,
        touches_os_lock_or_alloc: false,
        has_slot_drop_glue_cycle: false,
        manual_class: ManualLedgerClass::Small,
    });
    // hy_sha3_stream_checksum self=448B + keccak_p=192B = 640B (<= 1024B SMALL guard, > 512B NOSPLIT)
    g.insert(FnNode {
        symbol: "hy_sha3_stream_checksum",
        self_frame_bytes: 448,
        callees: vec!["keccak::keccak_p"],
        has_indirect_call: false,
        touches_os_lock_or_alloc: false,
        has_slot_drop_glue_cycle: false,
        manual_class: ManualLedgerClass::Small,
    });

    // 3. jsonbinding SMALL exports (touch TABLE Mutex + allocator/drop_glue):
    for (sym, frame, has_drop) in [
        ("hy_json_new_object", 64, true),
        ("hy_json_new_array", 64, true),
        ("hy_json_array_len", 48, false),
        ("hy_json_live_count", 48, false),
        ("hy_json_get_string", 128, true),
        ("hy_json_array_push_string", 112, true),
    ] {
        g.insert(FnNode {
            symbol: sym,
            self_frame_bytes: frame,
            callees: vec![],
            has_indirect_call: false,
            touches_os_lock_or_alloc: true,
            has_slot_drop_glue_cycle: has_drop,
            manual_class: ManualLedgerClass::Small,
        });
    }
    g.insert(FnNode {
        symbol: "hy_json_get_int",
        self_frame_bytes: 96,
        callees: vec!["serde_json::Value::index_into"],
        has_indirect_call: false,
        touches_os_lock_or_alloc: true,
        has_slot_drop_glue_cycle: false,
        manual_class: ManualLedgerClass::Small,
    });

    // 4. jsonbinding DEEP exports (recursive tree walk / clone / drop / serialize):
    for (sym, frame, manual) in [
        ("hy_json_parse", 240, ManualLedgerClass::Deep),
        ("hy_json_get_array", 144, ManualLedgerClass::Deep),
        ("hy_json_array_get", 160, ManualLedgerClass::Deep),
        ("hy_json_set", 208, ManualLedgerClass::Deep),
        ("hy_json_release", 64, ManualLedgerClass::Deep),
        ("hy_json_to_string_pretty", 144, ManualLedgerClass::Deep),
        ("hy_json_value_text", 144, ManualLedgerClass::Unlisted),
    ] {
        g.insert(FnNode {
            symbol: sym,
            self_frame_bytes: frame,
            callees: vec!["serde_json::recursive_tree_walk"],
            has_indirect_call: false,
            touches_os_lock_or_alloc: true,
            has_slot_drop_glue_cycle: true,
            manual_class: manual,
        });
    }

    // 5. regexbinding DEFAULT exports (16 KB static path + indirect DFA/NFA + parser recursion):
    g.insert(FnNode {
        symbol: "hy_regex_is_match",
        self_frame_bytes: 256,
        callees: vec!["regex_automata::compile_and_search"],
        has_indirect_call: true,
        touches_os_lock_or_alloc: true,
        has_slot_drop_glue_cycle: false,
        manual_class: ManualLedgerClass::Default,
    });
    g.insert(FnNode {
        symbol: "hy_regex_is_valid",
        self_frame_bytes: 64,
        callees: vec!["regex_automata::compile_and_search"],
        has_indirect_call: false,
        touches_os_lock_or_alloc: true,
        has_slot_drop_glue_cycle: false,
        manual_class: ManualLedgerClass::Default,
    });

    g
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn certifies_zero_call_leaves_for_nosplit_direct() {
        let g = build_measured_hyrt_graph();
        for (sym, expected_bytes) in [
            ("hy_nop", 0),
            ("hyrt_abi_word", 0),
            ("hyrt_stack_alignment_probe", 32),
        ] {
            let r = g.verify_export(sym);
            assert_eq!(
                r.certified_class,
                CertifiedClass::NosplitDirect {
                    max_stack_bytes: expected_bytes
                }
            );
            assert!(!r.ledger_hazard_detected);
        }
    }

    #[test]
    fn catches_sha3_digest256_exceeding_1024b_small_guard() {
        let g = build_measured_hyrt_graph();
        let digest = g.verify_export("hy_sha3_digest256");
        // 800 B (hy_sha3_digest256) + 48 B (update) + 192 B (keccak_p) = 1040 B > 1024 B SMALL guard
        assert_eq!(digest.happy_path_stack_bytes, Some(1040));
        assert!(matches!(
            digest.certified_class,
            CertifiedClass::DefaultGuarded {
                max_stack_bytes: 1040,
                ..
            }
        ));
        assert!(
            digest.ledger_hazard_detected,
            "verifier must flag that 1040 B exceeds the 1024 B SMALL guard in descriptors.md"
        );
    }

    #[test]
    fn classifies_sha3_stream_checksum_as_small_guarded_not_nosplit() {
        let g = build_measured_hyrt_graph();
        let stream = g.verify_export("hy_sha3_stream_checksum");
        // 448 B + 192 B = 640 B: fits inside 1024 B SMALL guard, but exceeds 512 B Go StackNosplit budget
        assert_eq!(stream.happy_path_stack_bytes, Some(640));
        assert!(matches!(
            stream.certified_class,
            CertifiedClass::SmallGuarded {
                max_stack_bytes: 640,
                ..
            }
        ));
        assert!(!stream.ledger_hazard_detected);
    }

    #[test]
    fn blocks_nosplit_on_json_small_leaves_due_to_os_mutex_and_alloc() {
        let g = build_measured_hyrt_graph();
        for sym in [
            "hy_json_new_object",
            "hy_json_new_array",
            "hy_json_get_int",
            "hy_json_get_string",
            "hy_json_array_len",
            "hy_json_array_push_string",
        ] {
            let r = g.verify_export(sym);
            assert!(
                matches!(r.certified_class, CertifiedClass::SmallGuarded { .. }),
                "expected SmallGuarded for {}, got {:?}",
                sym,
                r.certified_class
            );
            assert!(emit_certified_go_asm_stub(&r, "JsonStub", 16).is_err());
        }
    }

    #[test]
    fn rejects_recursive_json_exports_into_deep_pool() {
        let g = build_measured_hyrt_graph();
        for sym in [
            "hy_json_parse",
            "hy_json_get_array",
            "hy_json_array_get",
            "hy_json_set",
            "hy_json_release",
            "hy_json_to_string_pretty",
            "hy_json_value_text",
        ] {
            let r = g.verify_export(sym);
            assert!(
                matches!(r.certified_class, CertifiedClass::DeepPooled { .. }),
                "expected DeepPooled for {}, got {:?}",
                sym,
                r.certified_class
            );
        }
    }

    #[test]
    fn flags_regex_exports_exceeding_4096b_default_guard() {
        let g = build_measured_hyrt_graph();
        for sym in ["hy_regex_is_match", "hy_regex_is_valid"] {
            let r = g.verify_export(sym);
            assert!(matches!(r.certified_class, CertifiedClass::DeepPooled { .. }));
            assert!(
                r.ledger_hazard_detected,
                "verifier must flag regex export `{}` as exceeding the 4096 B DEFAULT guard",
                sym
            );
        }
    }

    #[test]
    fn detects_mutual_recursion_cycle_fail_closed() {
        let mut g = CallGraph::new();
        g.insert(FnNode {
            symbol: "fn_a",
            self_frame_bytes: 64,
            callees: vec!["fn_b"],
            has_indirect_call: false,
            touches_os_lock_or_alloc: false,
            has_slot_drop_glue_cycle: false,
            manual_class: ManualLedgerClass::Small,
        });
        g.insert(FnNode {
            symbol: "fn_b",
            self_frame_bytes: 64,
            callees: vec!["fn_a"],
            has_indirect_call: false,
            touches_os_lock_or_alloc: false,
            has_slot_drop_glue_cycle: false,
            manual_class: ManualLedgerClass::Small,
        });
        let r = g.verify_export("fn_a");
        assert!(matches!(r.certified_class, CertifiedClass::DeepPooled { .. }));
        assert!(r.ledger_hazard_detected);
    }

    #[test]
    fn emits_direct_go_asm_only_for_nosplit_certified_leaves() {
        let g = build_measured_hyrt_graph();
        let nop = g.verify_export("hy_nop");
        let asm = emit_certified_go_asm_stub(&nop, "NopDirect", 16).expect("hy_nop should emit");
        assert!(asm.contains("TEXT ·NopDirect(SB), NOSPLIT, $0-16"));
        assert!(!asm.contains("stackguard0"));

        let sha3 = g.verify_export("hy_sha3_stream_checksum");
        assert!(emit_certified_go_asm_stub(&sha3, "StreamChecksumDirect", 24).is_err());
    }
}
