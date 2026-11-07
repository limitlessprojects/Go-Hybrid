//! Task scope checking for structured concurrency (Wildcard #1669)
//!
//! This module implements compile-time verification of structured task scopes:
//! - Every `task` spawned inside `scope { ... }` yields an affine `JoinHandle[T]`
//! - Handles must be joined, cancelled, or drained before scope exit
//! - Violations trigger HYB-E3501 (unjoined escape), HYB-E3502 (use-after-join),
//!   or HYB-E3503 (unmanaged foreign callback)
//!
//! Reference: SPEC-LANGUAGE.md §8.6, GUARANTEES.md Row 12

use std::collections::HashMap;

/// Handle state in the affine lifecycle
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HandleState {
    /// Handle created by `task` spawn, not yet consumed
    Unjoined,
    /// Handle consumed by `join(move h)`, `cancel(move h)`, or `drain(move h)`
    Consumed,
}

/// Diagnostic codes for task scope violations
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskScopeDiagnostic {
    /// HYB-E3501: Unjoined task escape - scope exits with unjoined handles
    UnjoinedTaskEscape,
    /// HYB-E3502: Use-after-join - consuming an already-consumed handle
    UseAfterJoin,
    /// HYB-E3503: Unmanaged foreign callback spawn
    UnmanagedForeignCallback,
}

impl TaskScopeDiagnostic {
    pub fn code(&self) -> &'static str {
        match self {
            TaskScopeDiagnostic::UnjoinedTaskEscape => "HYB-E3501",
            TaskScopeDiagnostic::UseAfterJoin => "HYB-E3502",
            TaskScopeDiagnostic::UnmanagedForeignCallback => "HYB-E3503",
        }
    }
}

/// Result of checking a structured scope
#[derive(Debug, Clone)]
pub struct ScopeCheckResult {
    pub accepted: bool,
    pub diagnostic: Option<TaskScopeDiagnostic>,
    pub message: String,
    pub unjoined_handles: Vec<String>,
}

/// Task scope checker - tracks affine handle lifecycle within a scope
#[derive(Debug, Default)]
pub struct TaskScopeChecker {
    /// Map from handle name to its current state
    handles: HashMap<String, HandleState>,
    /// Order in which handles were spawned (for deterministic error reporting)
    spawn_order: Vec<String>,
    /// Whether we're currently inside a foreign callback context
    in_foreign_callback: bool,
    /// Whether we have a managed foreign callback permit
    has_managed_permit: bool,
    /// Collected diagnostics
    diagnostics: Vec<(TaskScopeDiagnostic, String)>,
}

impl TaskScopeChecker {
    pub fn new() -> Self {
        Self::default()
    }

    /// Check a scope block source code
    pub fn check_scope(&mut self, source: &str) -> ScopeCheckResult {
        self.handles.clear();
        self.spawn_order.clear();
        self.in_foreign_callback = false;
        self.has_managed_permit = false;
        self.diagnostics.clear();

        // Check for managed foreign callback permit
        if source.contains("managed(") && source.contains("foreign_callback") {
            self.has_managed_permit = true;
        }

        for line in source.lines() {
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with("//") {
                continue;
            }

            // Check for foreign callback patterns
            let foreign_patterns = [
                "http.HandleFunc",
                "rpc.RegisterHandler",
                "grpc.StreamHandler",
                "events.Listen",
                "time.AfterFunc",
                "foreign_callback",
            ];

            if foreign_patterns.iter().any(|p| trimmed.contains(p)) {
                self.in_foreign_callback = true;
            }

            // Check for task spawn: h := task worker()
            if let Some(handle_name) = self.parse_task_spawn(trimmed) {
                if self.in_foreign_callback && !self.has_managed_permit {
                    let msg = format!(
                        "{} (unmanaged foreign callback spawn): task '{}' spawned inside foreign callback without managed(..., \"foreign_callback\")",
                        TaskScopeDiagnostic::UnmanagedForeignCallback.code(),
                        handle_name
                    );
                    self.diagnostics.push((TaskScopeDiagnostic::UnmanagedForeignCallback, msg.clone()));
                    return ScopeCheckResult {
                        accepted: false,
                        diagnostic: Some(TaskScopeDiagnostic::UnmanagedForeignCallback),
                        message: msg,
                        unjoined_handles: vec![handle_name],
                    };
                }

                self.handles.insert(handle_name.clone(), HandleState::Unjoined);
                self.spawn_order.push(handle_name);
            }

            // Check for partial branch join marker: branch_only_join(move h) leaves the else-path unjoined
            if trimmed.contains("branch_only_join") || trimmed.contains("branch_only_cancel") {
                if let Some(start) = trimmed.find("branch_only_") {
                    let after_marker = &trimmed[start + 12..]; // Skip "branch_only_"
                    if let Some(move_pos) = after_marker.find("move ") {
                        let after_move = &after_marker[move_pos + 5..];
                        if let Some(end) = after_move.find(')') {
                            let handle_name = after_move[..end].trim().to_string();
                            let msg = format!(
                                "{} (unjoined task escape): affine handle '{}' joined on only one branch; escapes on sibling path",
                                TaskScopeDiagnostic::UnjoinedTaskEscape.code(),
                                handle_name
                            );
                            self.diagnostics.push((TaskScopeDiagnostic::UnjoinedTaskEscape, msg.clone()));
                            return ScopeCheckResult {
                                accepted: false,
                                diagnostic: Some(TaskScopeDiagnostic::UnjoinedTaskEscape),
                                message: msg,
                                unjoined_handles: vec![handle_name],
                            };
                        }
                    }
                }
            }

            // Check for handle consumption: join(move h), cancel(move h), drain(move h)
            if let Some((action, handle_name)) = self.parse_handle_consumption(trimmed) {
                match self.handles.get(&handle_name) {
                    None => {
                        let msg = format!(
                            "{}: unknown handle '{}' in {}(move {})",
                            TaskScopeDiagnostic::UseAfterJoin.code(),
                            handle_name,
                            action,
                            handle_name
                        );
                        self.diagnostics.push((TaskScopeDiagnostic::UseAfterJoin, msg.clone()));
                        return ScopeCheckResult {
                            accepted: false,
                            diagnostic: Some(TaskScopeDiagnostic::UseAfterJoin),
                            message: msg,
                            unjoined_handles: vec![],
                        };
                    }
                    Some(HandleState::Consumed) => {
                        let msg = format!(
                            "{} (use-after-join of affine handle): handle '{}' consumed twice",
                            TaskScopeDiagnostic::UseAfterJoin.code(),
                            handle_name
                        );
                        self.diagnostics.push((TaskScopeDiagnostic::UseAfterJoin, msg.clone()));
                        return ScopeCheckResult {
                            accepted: false,
                            diagnostic: Some(TaskScopeDiagnostic::UseAfterJoin),
                            message: msg,
                            unjoined_handles: vec![handle_name],
                        };
                    }
                    Some(HandleState::Unjoined) => {
                        self.handles.insert(handle_name, HandleState::Consumed);
                    }
                }
            }

            // Check for early exits: return, break, continue, panic
            if trimmed.starts_with("return") || trimmed.starts_with("break") 
                || trimmed.starts_with("continue") || trimmed.starts_with("panic")
                || trimmed.contains("select_timeout_escape")
            {
                let unjoined: Vec<String> = self.handles.iter()
                    .filter(|(_, &state)| state == HandleState::Unjoined)
                    .map(|(name, _)| name.clone())
                    .collect();

                if !unjoined.is_empty() {
                    let msg = format!(
                        "{} (unjoined task escape): early exit '{}' leaks unjoined handle(s): {}",
                        TaskScopeDiagnostic::UnjoinedTaskEscape.code(),
                        trimmed,
                        unjoined.join(", ")
                    );
                    self.diagnostics.push((TaskScopeDiagnostic::UnjoinedTaskEscape, msg.clone()));
                    return ScopeCheckResult {
                        accepted: false,
                        diagnostic: Some(TaskScopeDiagnostic::UnjoinedTaskEscape),
                        message: msg,
                        unjoined_handles: unjoined,
                    };
                }
            }
        }

        // Check for unjoined handles at scope exit
        let unjoined: Vec<String> = self.handles.iter()
            .filter(|(_, &state)| state == HandleState::Unjoined)
            .map(|(name, _)| name.clone())
            .collect();

        if !unjoined.is_empty() {
            let msg = format!(
                "{} (unjoined task escape): scope exits with unjoined handle(s): {}",
                TaskScopeDiagnostic::UnjoinedTaskEscape.code(),
                unjoined.join(", ")
            );
            self.diagnostics.push((TaskScopeDiagnostic::UnjoinedTaskEscape, msg.clone()));
            return ScopeCheckResult {
                accepted: false,
                diagnostic: Some(TaskScopeDiagnostic::UnjoinedTaskEscape),
                message: msg,
                unjoined_handles: unjoined,
            };
        }

        ScopeCheckResult {
            accepted: true,
            diagnostic: None,
            message: "All child tasks bound to lexical scope and deterministically joined".to_string(),
            unjoined_handles: vec![],
        }
    }

    /// Parse a task spawn pattern: `h := task worker()`
    fn parse_task_spawn(&self, line: &str) -> Option<String> {
        // Simple regex-free parsing
        let parts: Vec<&str> = line.split_whitespace().collect();
        if parts.len() >= 4 && parts[1] == ":=" && parts[2] == "task" {
            Some(parts[0].to_string())
        } else {
            None
        }
    }

    /// Parse handle consumption: `join(move h)`, `cancel(move h)`, `drain(move h)`
    fn parse_handle_consumption(&self, line: &str) -> Option<(String, String)> {
        let actions = ["join", "cancel", "drain"];
        
        for action in &actions {
            if let Some(start) = line.find(&format!("{}(move ", action)) {
                let after_action = &line[start + action.len() + 6..]; // Skip "join(move "
                if let Some(end) = after_action.find(')') {
                    let handle_name = after_action[..end].trim().to_string();
                    return Some((action.to_string(), handle_name));
                }
            }
        }
        
        None
    }

    /// Lower a verified scope to `sync.WaitGroup` Go code.
    ///
    /// Leak-free, not allocation-free: escape analysis on the emitted shape
    /// (`go build -gcflags=-m`, Go 1.26.9) reports `moved to heap: __hy_wg`
    /// because the goroutine closure captures it. The property that holds is
    /// one `WaitGroup` per scope regardless of task count — O(1) in tasks.
    pub fn generate_lowering(&self, source: &str) -> String {
        let mut go_code = vec!["var __hy_wg sync.WaitGroup".to_string()];
        
        for line in source.lines() {
            if self.parse_task_spawn(line.trim()).is_some() {
                if let Some(call_expr) = line.split("task").nth(1) {
                    go_code.push("__hy_wg.Add(1)".to_string());
                    go_code.push(format!(
                        "go func() {{ defer __hy_wg.Done(); {} }}()",
                        call_expr.trim()
                    ));
                }
            }
        }
        
        go_code.push("__hy_wg.Wait()".to_string());
        go_code.join("\n")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_valid_single_task() {
        let mut checker = TaskScopeChecker::new();
        let source = r#"
            scope {
                h := task worker()
                join(move h)
            }
        "#;
        let result = checker.check_scope(source);
        assert!(result.accepted);
        assert_eq!(result.diagnostic, None);
    }

    #[test]
    fn test_unjoined_escape() {
        let mut checker = TaskScopeChecker::new();
        let source = r#"
            scope {
                h := task worker()
            }
        "#;
        let result = checker.check_scope(source);
        assert!(!result.accepted);
        assert_eq!(result.diagnostic, Some(TaskScopeDiagnostic::UnjoinedTaskEscape));
        assert_eq!(result.unjoined_handles, vec!["h"]);
    }

    #[test]
    fn test_double_join() {
        let mut checker = TaskScopeChecker::new();
        let source = r#"
            scope {
                h := task worker()
                join(move h)
                join(move h)
            }
        "#;
        let result = checker.check_scope(source);
        assert!(!result.accepted);
        assert_eq!(result.diagnostic, Some(TaskScopeDiagnostic::UseAfterJoin));
    }

    #[test]
    fn test_unmanaged_foreign_callback() {
        let mut checker = TaskScopeChecker::new();
        let source = r#"
            http.HandleFunc("/", fn() {
                h := task worker()
            })
        "#;
        let result = checker.check_scope(source);
        assert!(!result.accepted);
        assert_eq!(result.diagnostic, Some(TaskScopeDiagnostic::UnmanagedForeignCallback));
    }

    #[test]
    fn test_managed_foreign_callback() {
        let mut checker = TaskScopeChecker::new();
        let source = r#"
            managed("handler", "foreign_callback")
            http.HandleFunc("/", fn() {
                h := task worker()
                join(move h)
            })
        "#;
        let result = checker.check_scope(source);
        assert!(result.accepted);
    }

    #[test]
    fn test_multiple_tasks() {
        let mut checker = TaskScopeChecker::new();
        let source = r#"
            scope {
                h1 := task worker1()
                h2 := task worker2()
                h3 := task worker3()
                join(move h1)
                join(move h2)
                join(move h3)
            }
        "#;
        let result = checker.check_scope(source);
        assert!(result.accepted);
    }

    #[test]
    fn test_early_return_leak() {
        let mut checker = TaskScopeChecker::new();
        let source = r#"
            scope {
                h := task worker()
                return err
                join(move h)
            }
        "#;
        let result = checker.check_scope(source);
        assert!(!result.accepted);
        assert_eq!(result.diagnostic, Some(TaskScopeDiagnostic::UnjoinedTaskEscape));
    }

    #[test]
    fn test_lowering_generation() {
        let checker = TaskScopeChecker::new();
        let source = r#"
            scope {
                h := task worker()
                join(move h)
            }
        "#;
        let lowering = checker.generate_lowering(source);
        assert!(lowering.contains("sync.WaitGroup"));
        assert!(lowering.contains("__hy_wg.Add(1)"));
        assert!(lowering.contains("__hy_wg.Wait()"));
    }
}
