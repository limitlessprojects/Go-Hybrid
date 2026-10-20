use hy_check::check_file;
use hy_diag::Diagnostic;
use hy_syntax::parse_file;

fn check(src: &str) -> Vec<Diagnostic> {
    let ast = parse_file("test.hy", src).expect("parses");
    check_file(&ast).diagnostics
}

#[test]
fn test_repro1_slice_reslice_rebind_rejected() {
    let src = r#"
package main
import "fmt"

struct service {
    replicas int
    tag      string
}

fn main() {
    xs := []service{{replicas: 1, tag: "a"}}
    s := &xs[0]
    xs = xs[:0]
    s.replicas = 5
    fmt.println(len(xs))
}
"#;
    let diags = check(src);
    assert_eq!(diags.len(), 1, "expected 1 diagnostic, got {:?}", diags);
    assert_eq!(diags[0].code, "HYB-E3102");
    assert!(
        diags[0].message.contains("slice rebind invalidates the live slot loan `s`"),
        "unexpected message: {}",
        diags[0].message
    );
}

#[test]
fn test_repro2_slice_subslice_rebind_rejected() {
    let src = r#"
package main
import "fmt"

struct service {
    replicas int
    tag      string
}

fn main() {
    xs := []service{{replicas: 1, tag: "a"}, {replicas: 2, tag: "b"}}
    s := &xs[0]
    xs = xs[1:]
    s.replicas = 5
    fmt.println(xs[0].replicas)
}
"#;
    let diags = check(src);
    assert_eq!(diags.len(), 1, "expected 1 diagnostic, got {:?}", diags);
    assert_eq!(diags[0].code, "HYB-E3102");
    assert!(
        diags[0].message.contains("slice rebind invalidates the live slot loan `s`"),
        "unexpected message: {}",
        diags[0].message
    );
}

#[test]
fn test_repro3_slice_append_realloc_rebind_rejected() {
    let src = r#"
package main
import "fmt"

struct service {
    replicas int
    tag      string
}

fn main() {
    xs := make([]service, 0, 1)
    xs = append(xs, service{replicas: 1, tag: "a"})
    s := &xs[0]
    xs = append(xs, service{replicas: 2, tag: "b"})
    s.replicas = 5
    fmt.println(xs[0].replicas)
}
"#;
    let diags = check(src);
    assert_eq!(diags.len(), 1, "expected 1 diagnostic, got {:?}", diags);
    assert_eq!(diags[0].code, "HYB-E3102");
    assert!(
        diags[0].message.contains("slice rebind invalidates the live slot loan `s`"),
        "unexpected message: {}",
        diags[0].message
    );
}

#[test]
fn test_map_whole_binding_rebind_rejected() {
    let src = r#"
package main
import "fmt"

struct service {
    replicas int
    tag      string
}

fn main() {
    m := map[string]service{"a": {replicas: 1, tag: "a"}}
    s := &m["a"]
    m = map[string]service{}
    s.replicas = 5
    fmt.println(s.replicas)
}
"#;
    let diags = check(src);
    assert_eq!(diags.len(), 1, "expected 1 diagnostic, got {:?}", diags);
    assert_eq!(diags[0].code, "HYB-E3102");
    assert!(
        diags[0].message.contains("map rebind invalidates the live slot loan `s`"),
        "unexpected message: {}",
        diags[0].message
    );
}

#[test]
fn test_struct_field_container_rebind_rejected() {
    let src = r#"
package main
import "fmt"

struct service {
    replicas int
    tag      string
}

struct env {
    services map[string]service
}

fn main() {
    e := env{services: map[string]service{"a": {replicas: 1, tag: "a"}}}
    svc := &e.services["a"]
    e = env{services: map[string]service{}}
    svc.replicas = 5
    fmt.println(svc.replicas)
}
"#;
    let diags = check(src);
    assert_eq!(diags.len(), 1, "expected 1 diagnostic, got {:?}", diags);
    assert_eq!(diags[0].code, "HYB-E3102");
    assert!(
        diags[0].message.contains("map rebind invalidates the live slot loan `svc`"),
        "unexpected message: {}",
        diags[0].message
    );
}

#[test]
fn test_slice_rebind_after_last_use_accepted() {
    let src = r#"
package main
import "fmt"

struct service {
    replicas int
    tag      string
}

fn main() {
    xs := []service{{replicas: 1, tag: "a"}}
    s := &xs[0]
    s.replicas = 5
    xs = xs[:0]
    fmt.println(len(xs))
}
"#;
    let diags = check(src);
    assert!(diags.is_empty(), "expected 0 diagnostics, got {:?}", diags);
}

#[test]
fn test_slice_other_element_index_write_accepted() {
    let src = r#"
package main
import "fmt"

struct service {
    replicas int
    tag      string
}

fn main() {
    xs := []service{{replicas: 1, tag: "a"}, {replicas: 2, tag: "b"}}
    s := &xs[0]
    xs[1].replicas = 9
    s.replicas = 5
    fmt.println(xs[0].replicas, xs[1].replicas)
}
"#;
    let diags = check(src);
    assert!(diags.is_empty(), "expected 0 diagnostics, got {:?}", diags);
}

#[test]
fn test_loan_field_mutation_itself_accepted() {
    let src = r#"
package main
import "fmt"

struct service {
    replicas int
    tag      string
}

fn main() {
    xs := []service{{replicas: 1, tag: "a"}}
    s := &xs[0]
    s.replicas = 5
    s.tag = "updated"
    fmt.println(s.replicas, s.tag)
}
"#;
    let diags = check(src);
    assert!(diags.is_empty(), "expected 0 diagnostics, got {:?}", diags);
}
