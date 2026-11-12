//! Standalone Static Call-Graph Allocator & Stack Oracle
//! Wildcard Spike 22: Dual-Heap Allocation Certification (#394)
//! Scoped model: zero dependencies, compiles cleanly with `rustc --test`.

use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocVerdict {
    AllocFree,
    MayAlloc,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GuardClass {
    NosplitDirect, // <= 512 B
    Small,         // <= 1,024 B
    DefaultGuard,  // <= 4,096 B
    DeepStack,     // Pool stack (1 MiB)
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExportClassification {
    pub name: String,
    pub self_frame: usize,
    pub max_stack: usize,
    pub is_cyclic: bool,
    pub reaches_rust_alloc: bool,
    pub verdict: AllocVerdict,
    pub recommended_guard: GuardClass,
}

pub struct FunctionNode {
    pub name: String,
    pub frame_size: usize,
    pub callees: Vec<String>,
    pub is_allocator_symbol: bool,
}

pub struct CallGraph {
    nodes: HashMap<String, FunctionNode>,
}

impl CallGraph {
    pub fn new() -> Self {
        Self {
            nodes: HashMap::new(),
        }
    }

    pub fn add_function(&mut self, name: &str, frame_size: usize, callees: &[&str]) {
        let is_alloc = name == "__rust_alloc"
            || name == "__rust_realloc"
            || name == "alloc::alloc::alloc"
            || name == "process_heap_alloc";
        self.nodes.insert(
            name.to_string(),
            FunctionNode {
                name: name.to_string(),
                frame_size,
                callees: callees.iter().map(|s| s.to_string()).collect(),
                is_allocator_symbol: is_alloc,
            },
        );
    }

    pub fn reaches_allocator(&self, entry: &str) -> bool {
        let mut visited = HashSet::new();
        let mut stack = vec![entry.to_string()];

        while let Some(current) = stack.pop() {
            if !visited.insert(current.clone()) {
                continue;
            }
            if let Some(node) = self.nodes.get(&current) {
                if node.is_allocator_symbol {
                    return true;
                }
                for callee in &node.callees {
                    stack.push(callee.clone());
                }
            }
        }
        false
    }

    pub fn compute_stack_bounds(&self, entry: &str) -> (usize, bool) {
        let mut visited = HashSet::new();
        let mut on_stack = HashSet::new();
        let mut is_cyclic = false;

        fn dfs(
            graph: &CallGraph,
            current: &str,
            visited: &mut HashSet<String>,
            on_stack: &mut HashSet<String>,
            is_cyclic: &mut bool,
        ) -> usize {
            if on_stack.contains(current) {
                *is_cyclic = true;
                return 0;
            }
            if let Some(node) = graph.nodes.get(current) {
                on_stack.insert(current.to_string());
                visited.insert(current.to_string());

                let mut max_child = 0;
                for callee in &node.callees {
                    let child_cost = dfs(graph, callee, visited, on_stack, is_cyclic);
                    if child_cost > max_child {
                        max_child = child_cost;
                    }
                }

                on_stack.remove(current);
                node.frame_size + max_child
            } else {
                0
            }
        }

        let max_stack = dfs(self, entry, &mut visited, &mut on_stack, &mut is_cyclic);
        (max_stack, is_cyclic)
    }

    pub fn classify(&self, export_name: &str) -> Option<ExportClassification> {
        let node = self.nodes.get(export_name)?;
        let (max_stack, is_cyclic) = self.compute_stack_bounds(export_name);
        let reaches_alloc = self.reaches_allocator(export_name);

        let verdict = if !reaches_alloc && !is_cyclic {
            AllocVerdict::AllocFree
        } else {
            AllocVerdict::MayAlloc
        };

        let recommended_guard = if max_stack <= 512 && verdict == AllocVerdict::AllocFree {
            GuardClass::NosplitDirect
        } else if max_stack <= 1024 {
            GuardClass::Small
        } else if max_stack <= 4096 {
            GuardClass::DefaultGuard
        } else {
            GuardClass::DeepStack
        };

        Some(ExportClassification {
            name: export_name.to_string(),
            self_frame: node.frame_size,
            max_stack,
            is_cyclic,
            reaches_rust_alloc: reaches_alloc,
            verdict,
            recommended_guard,
        })
    }

    /// Load the standard `runtime/rust/hyrt` call-graph model as extracted from `main`.
    pub fn load_hyrt_model() -> Self {
        let mut g = Self::new();

        // Allocator sinks
        g.add_function("__rust_alloc", 0, &[]);
        g.add_function("__rust_realloc", 0, &[]);

        // Leaf exports (0 allocations, bounded stack)
        g.add_function("hy_nop", 8, &[]);
        g.add_function("hyrt_abi_word", 8, &[]);
        g.add_function("hyrt_stack_alignment_probe", 48, &[]);

        // SHA3 exports
        g.add_function("keccak_f1600_p", 192, &[]);
        g.add_function("hy_sha3_digest256", 624, &["keccak_f1600_p"]);
        g.add_function("hy_sha3_stream_checksum", 280, &["keccak_f1600_p"]);

        // JSON read-only fast-paths
        g.add_function("hy_json_array_len", 80, &[]);
        g.add_function("table_lookup_read", 112, &[]);
        g.add_function("hy_json_get_int", 128, &["table_lookup_read"]);

        // JSON mutating / allocating paths
        g.add_function("serde_json_alloc_node", 64, &["__rust_alloc"]);
        g.add_function("table_insert_mut", 128, &["serde_json_alloc_node"]);
        g.add_function("hy_json_new_object", 96, &["table_insert_mut"]);
        g.add_function("hy_json_parse", 256, &["serde_json_alloc_node", "table_insert_mut"]);

        // Regex unhoisted (dynamic compilation on call site)
        g.add_function("regex_parser_rec", 512, &["regex_parser_rec", "__rust_alloc"]);
        g.add_function("hy_regex_is_match_dynamic", 304, &["regex_parser_rec"]);

        // Regex hoisted (precompiled automaton view)
        g.add_function("pikevm_exec_scratch", 416, &[]);
        g.add_function("hy_regex_is_match_hoisted", 96, &["pikevm_exec_scratch"]);

        g
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_leaf_nop_is_alloc_free_nosplit() {
        let g = CallGraph::load_hyrt_model();
        let c = g.classify("hy_nop").expect("hy_nop exists");
        assert_eq!(c.verdict, AllocVerdict::AllocFree);
        assert_eq!(c.max_stack, 8);
        assert_eq!(c.recommended_guard, GuardClass::NosplitDirect);
        assert!(!c.reaches_rust_alloc);
    }

    #[test]
    fn test_sha3_digest256_is_alloc_free_and_small_guard() {
        let g = CallGraph::load_hyrt_model();
        let c = g.classify("hy_sha3_digest256").expect("sha3 exists");
        assert_eq!(c.verdict, AllocVerdict::AllocFree);
        assert_eq!(c.max_stack, 816);
        assert_eq!(c.recommended_guard, GuardClass::Small);
        assert!(!c.reaches_rust_alloc);
    }

    #[test]
    fn test_json_get_int_is_alloc_free() {
        let g = CallGraph::load_hyrt_model();
        let c = g.classify("hy_json_get_int").expect("json_get_int exists");
        assert_eq!(c.verdict, AllocVerdict::AllocFree);
        assert_eq!(c.max_stack, 240);
        assert_eq!(c.recommended_guard, GuardClass::NosplitDirect);
    }

    #[test]
    fn test_json_parse_fails_closed_as_may_alloc() {
        let g = CallGraph::load_hyrt_model();
        let c = g.classify("hy_json_parse").expect("json_parse exists");
        assert_eq!(c.verdict, AllocVerdict::MayAlloc);
        assert!(c.reaches_rust_alloc);
    }

    #[test]
    fn test_dynamic_regex_detects_cycle_and_alloc() {
        let g = CallGraph::load_hyrt_model();
        let c = g.classify("hy_regex_is_match_dynamic").expect("dynamic regex exists");
        assert_eq!(c.verdict, AllocVerdict::MayAlloc);
        assert!(c.reaches_rust_alloc);
        assert!(c.is_cyclic);
    }

    #[test]
    fn test_hoisted_regex_is_alloc_free_small() {
        let g = CallGraph::load_hyrt_model();
        let c = g.classify("hy_regex_is_match_hoisted").expect("hoisted regex exists");
        assert_eq!(c.verdict, AllocVerdict::AllocFree);
        assert_eq!(c.max_stack, 512);
        assert_eq!(c.recommended_guard, GuardClass::NosplitDirect);
        assert!(!c.reaches_rust_alloc);
    }
}
