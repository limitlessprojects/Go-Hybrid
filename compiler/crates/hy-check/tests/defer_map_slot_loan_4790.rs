use hy_check::check_file;
use hy_syntax::parse_file;

fn check_codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("test source must parse");
    let report = check_file(&ast);
    report.diagnostics.into_iter().map(|d| d.code).collect()
}

#[test]
fn defer_closure_using_map_slot_loan_rejects_delete_with_e3102() {
    let src = "\
struct service { replicas int }
fn run(services map[string]service) {
    svc := &services[\"a\"]
    defer fn() {
        svc.replicas = 5
    }()
    delete(services, \"a\")
}
";
    let codes = check_codes(src);
    assert!(
        codes.iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 when delete follows defer using O8 map-slot loan, got {:?}",
        codes
    );
}

#[test]
fn defer_call_and_clear_and_insert_and_alias_and_prebound_closure_reject_with_e3102() {
    let direct_call = "\
struct service { replicas int }
fn bump(s *service) { s.replicas = 5 }
fn run(services map[string]service) {
    svc := &services[\"a\"]
    defer bump(svc)
    delete(services, \"a\")
}
";
    assert!(
        check_codes(direct_call).iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 for defer bump(svc) followed by delete"
    );

    let clear_map = "\
struct service { replicas int }
fn run(services map[string]service) {
    svc := &services[\"a\"]
    defer fn() { svc.replicas = 5 }()
    clear(services)
}
";
    assert!(
        check_codes(clear_map).iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 for defer using loan followed by clear"
    );

    let insert_map = "\
struct service { replicas int }
fn bump(s *service) { s.replicas = 5 }
fn run(services map[string]service) {
    svc := &services[\"a\"]
    defer bump(svc)
    services[\"b\"] = service{replicas: 2}
}
";
    assert!(
        check_codes(insert_map).iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 for defer using loan followed by map insert"
    );

    let alias_defer = "\
struct service { replicas int }
fn bump(s *service) { s.replicas = 5 }
fn run(services map[string]service) {
    svc := &services[\"a\"]
    p := svc
    defer bump(p)
    delete(services, \"a\")
}
";
    assert!(
        check_codes(alias_defer).iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 for defer using alias of map-slot loan"
    );

    let prebound_closure = "\
struct service { replicas int }
fn run(services map[string]service) {
    svc := &services[\"a\"]
    f := fn() { svc.replicas = 5 }
    defer f()
    delete(services, \"a\")
}
";
    assert!(
        check_codes(prebound_closure).iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 for defer calling pre-bound closure capturing map-slot loan"
    );

    let nested_defer = "\
struct service { replicas int }
fn bump(s *service) { s.replicas = 5 }
fn run(services map[string]service, flag bool) {
    svc := &services[\"a\"]
    if flag {
        defer bump(svc)
    }
    delete(services, \"a\")
}
";
    assert!(
        check_codes(nested_defer).iter().any(|c| c == "HYB-E3102"),
        "expected HYB-E3102 for defer inside if-block followed by delete"
    );
}

#[test]
fn unrelated_or_shadowed_defer_and_non_deferred_loan_stay_accepted() {
    let unrelated_defer = "\
struct service { replicas int }
fn run(services map[string]service) {
    svc := &services[\"a\"]
    svc.replicas = 2
    delete(services, \"a\")
    defer fn() { _ = 1 }()
}
";
    assert!(
        check_codes(unrelated_defer).is_empty(),
        "unrelated defer after loan last use must stay accepted"
    );

    let shadowed_defer = "\
struct service { replicas int }
fn run(services map[string]service) {
    svc := &services[\"a\"]
    svc.replicas = 2
    defer fn(svc int) { _ = svc }(1)
    delete(services, \"a\")
}
";
    assert!(
        check_codes(shadowed_defer).is_empty(),
        "shadowed parameter in defer closure must stay accepted"
    );

    let no_defer = "\
struct service { replicas int }
fn run(services map[string]service) {
    svc := &services[\"a\"]
    svc.replicas = 2
    delete(services, \"a\")
}
";
    assert!(
        check_codes(no_defer).is_empty(),
        "non-deferred loan finished before delete must stay accepted"
    );
}
