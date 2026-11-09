//! Integration tests for structured task scopes (Wildcard #1669)
//!
//! Tests the full 25-pattern adversarial corpus plus 10 valid patterns.
//! Reference: spikes/w1669-structured-task-scopes/tests/test_w1669_scopes.py

use hy_check::task_scope::{TaskScopeChecker, TaskScopeDiagnostic};

#[test]
fn test_adversarial_corpus_100_percent_separation() {
    let mut checker = TaskScopeChecker::new();
    
    // 25 leaky patterns - all must be rejected
    let leaky_patterns = vec![
        // L1-L5: Basic unjoined escapes
        ("L1", "scope {\n  h1 := task fetch_chunk()\n}"),
        ("L2", "scope {\n  h1 := task fetch_a()\n  h2 := task fetch_b()\n  h3 := task fetch_c()\n}"),
        ("L3", "scope {\n  h1 := task fetch_a()\n  h2 := task fetch_b()\n  join(move h1)\n}"),
        ("L4", "scope {\n  scope {\n    h1 := task inner_worker()\n  }\n}"),
        ("L5", "scope {\n  if cond {\n    h1 := task conditional_worker()\n  }\n}"),
        
        // L6-L10: Early exit leaks
        ("L6", "scope {\n  h1 := task fetch_chunk()\n  return err\n  join(move h1)\n}"),
        ("L7", "scope {\n  h1 := task worker()\n  break\n  join(move h1)\n}"),
        ("L8", "scope {\n  h1 := task worker()\n  continue\n  join(move h1)\n}"),
        ("L9", "scope {\n  h1 := task query_db()\n  select_timeout_escape()\n  join(move h1)\n}"),
        ("L10", "scope {\n  h1 := task worker()\n  panic(\"error\")\n  join(move h1)\n}"),
        
        // L11-L15: Branch partial joins
        ("L11", "scope {\n  h1 := task compute()\n  branch_only_join(move h1)\n}"),
        ("L12", "scope {\n  h1 := task compute()\n  branch_only_cancel(move h1)\n}"),
        ("L13", "scope {\n  if cond {\n    h1 := task conditional_worker()\n  }\n}"),
        ("L14", "scope {\n  if x == 1 {\n    h1 := task worker1()\n  }\n  if x == 2 {\n    h2 := task worker2()\n  }\n}"),
        ("L15", "scope {\n  h1 := task worker()\n  for {\n    if done { break }\n  }\n}"),
        
        // L16-L20: Double consumption violations
        ("L16", "scope {\n  h1 := task compute()\n  join(move h1)\n  join(move h1)\n}"),
        ("L17", "scope {\n  h1 := task compute()\n  join(move h1)\n  cancel(move h1)\n}"),
        ("L18", "scope {\n  h1 := task compute()\n  cancel(move h1)\n  drain(move h1)\n}"),
        ("L19", "scope {\n  h1 := task compute()\n  drain(move h1)\n  join(move h1)\n}"),
        ("L20", "scope {\n  h1 := task compute()\n  join(move h1)\n  cancel(move h1)\n  drain(move h1)\n}"),
        
        // L21-L25: Foreign callback boundary violations
        ("L21", "http.HandleFunc(\"/\", fn() {\n  h1 := task bg_job()\n})"),
        ("L22", "rpc.RegisterHandler(\"method\", fn() {\n  h1 := task handler()\n})"),
        ("L23", "grpc.StreamHandler(\"service\", fn() {\n  h1 := task stream_worker()\n})"),
        ("L24", "events.Listen(\"click\", fn() {\n  h1 := task event_handler()\n})"),
        ("L25", "time.AfterFunc(5*time.Second, fn() {\n  h1 := task timer_handler()\n})"),
    ];
    
    for (name, source) in &leaky_patterns {
        let result = checker.check_scope(source);
        assert!(!result.accepted, "{} should be rejected", name);
        assert!(result.diagnostic.is_some(), "{} should have a diagnostic", name);
    }
    
    // 10 valid patterns - all must be accepted
    let valid_patterns = vec![
        ("V1", "scope {\n  h1 := task fetch_chunk()\n  join(move h1)\n}"),
        ("V2", "scope {\n  h1 := task part_a()\n  h2 := task part_b()\n  join(move h1)\n  join(move h2)\n}"),
        ("V3", "scope {\n  h1 := task stream_events()\n  cancel(move h1)\n}"),
        ("V4", "scope {\n  h1 := task worker_loop()\n  drain(move h1)\n  return ok\n}"),
        ("V5", "scope {\n  h1 := task primary()\n  h2 := task backup()\n  join(move h1)\n  cancel(move h2)\n}"),
        ("V6", "managed(\"net/http handler\", \"foreign_callback\")\nhttp.HandleFunc(\"/\", fn() {\n  h1 := task bg_job()\n  join(move h1)\n})"),
        ("V7", "scope {\n  h1 := task outer()\n  scope {\n    h2 := task inner()\n    join(move h2)\n  }\n  join(move h1)\n}"),
        ("V8", "scope {\n  h1 := task worker1()\n  h2 := task worker2()\n  join(move h1)\n  cancel(move h2)\n}"),
        ("V9", "scope {\n  for i := 0; i < n; i++ {\n    h := task worker(i)\n    join(move h)\n  }\n}"),
        ("V10", "scope {\n  h1 := task worker1()\n  h2 := task worker2()\n  h3 := task worker3()\n  h4 := task worker4()\n  h5 := task worker5()\n  join(move h1)\n  join(move h2)\n  join(move h3)\n  join(move h4)\n  join(move h5)\n}"),
    ];
    
    for (name, source) in &valid_patterns {
        let result = checker.check_scope(source);
        assert!(result.accepted, "{} should be accepted: {:?}", name, result.message);
        assert_eq!(result.diagnostic, None, "{} should have no diagnostic", name);
    }
}

#[test]
fn test_diagnostic_codes() {
    let mut checker = TaskScopeChecker::new();
    
    // HYB-E3501: Unjoined task escape
    let result = checker.check_scope("scope {\n  h := task worker()\n}");
    assert_eq!(result.diagnostic, Some(TaskScopeDiagnostic::UnjoinedTaskEscape));
    
    // HYB-E3502: Use-after-join
    let result = checker.check_scope("scope {\n  h := task worker()\n  join(move h)\n  join(move h)\n}");
    assert_eq!(result.diagnostic, Some(TaskScopeDiagnostic::UseAfterJoin));
    
    // HYB-E3503: Unmanaged foreign callback
    let result = checker.check_scope("http.HandleFunc(\"/\", fn() {\n  h := task worker()\n})");
    assert_eq!(result.diagnostic, Some(TaskScopeDiagnostic::UnmanagedForeignCallback));
}

#[test]
fn test_lowering_shape() {
    let checker = TaskScopeChecker::new();
    let source = "scope {\n  h1 := task worker1()\n  h2 := task worker2()\n  join(move h1)\n  join(move h2)\n}";
    let lowering = checker.generate_lowering(source);
    
    assert!(lowering.contains("var __hy_wg sync.WaitGroup"));
    assert!(lowering.contains("__hy_wg.Add(1)"));
    assert!(lowering.contains("go func()"));
    assert!(lowering.contains("defer __hy_wg.Done()"));
    assert!(lowering.contains("__hy_wg.Wait()"));
}

#[test]
fn test_nested_scopes() {
    let mut checker = TaskScopeChecker::new();
    
    // Valid nested scopes
    let source = r#"
        scope {
            h1 := task outer()
            scope {
                h2 := task inner()
                join(move h2)
            }
            join(move h1)
        }
    "#;
    let result = checker.check_scope(source);
    assert!(result.accepted);
    
    // Invalid: leak in inner scope
    let source = r#"
        scope {
            h1 := task outer()
            scope {
                h2 := task inner()
                // h2 not joined
            }
            join(move h1)
        }
    "#;
    let result = checker.check_scope(source);
    assert!(!result.accepted);
    assert_eq!(result.diagnostic, Some(TaskScopeDiagnostic::UnjoinedTaskEscape));
}

#[test]
fn test_mixed_consumption_methods() {
    let mut checker = TaskScopeChecker::new();
    let source = r#"
        scope {
            h1 := task worker1()
            h2 := task worker2()
            h3 := task worker3()
            join(move h1)
            cancel(move h2)
            drain(move h3)
        }
    "#;
    let result = checker.check_scope(source);
    assert!(result.accepted);
}

#[test]
fn test_error_messages() {
    let mut checker = TaskScopeChecker::new();
    
    let result = checker.check_scope("scope {\n  h := task worker()\n}");
    assert!(result.message.contains("HYB-E3501"));
    assert!(result.message.contains("unjoined task escape"));
    assert!(result.unjoined_handles.contains(&"h".to_string()));
    
    let result = checker.check_scope("scope {\n  h := task worker()\n  join(move h)\n  join(move h)\n}");
    assert!(result.message.contains("HYB-E3502"));
    assert!(result.message.contains("use-after-join"));
}
