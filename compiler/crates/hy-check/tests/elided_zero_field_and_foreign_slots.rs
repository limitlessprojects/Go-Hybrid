//! Regression tests for `elided_literal.rs` zero-field structs (`struct empty {}`),
//! foreign map-underlying types (`http.header`, `url.values`), and duplicate field
//! detection on foreign/unknown struct literals (`HYB-E1002`, GUARANTEES Row 13
//! `BUILDABLE`).

use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let file = parse_file("test.hy", src).expect("parses");
    check_file(&file)
        .diagnostics
        .into_iter()
        .map(|d| d.code.to_string())
        .collect()
}

#[test]
fn accepts_empty_elided_zero_field_structs_and_rejects_unknown_fields() {
    assert!(
        codes(
            r#"import "fmt"
struct empty {}
fn main() {
    xs := []empty{{}}
    ys := []*empty{{}}
    m := map[string]empty{"a": {}}
    fmt.println(len(xs), len(ys), len(m))
}
"#,
        )
        .is_empty(),
        "empty elided zero-field struct literal {{}} in slice/ptr-slice/map must pass with 0 diagnostics"
    );

    assert_eq!(
        codes(
            r#"import "fmt"
struct empty {}
fn main() {
    xs := []empty{{x: 1}}
    fmt.println(len(xs))
}
"#,
        ),
        vec!["HYB-E1002"],
        "non-empty elided literal on zero-field struct must reject unknown field with HYB-E1002"
    );
}

#[test]
fn accepts_empty_elided_foreign_map_types_and_rejects_struct_field_syntax() {
    assert!(
        codes(
            r#"import "fmt"
import "net/http"
import "net/url"
fn main() {
    xs := []http.header{{}}
    ys := []url.values{{}}
    fmt.println(len(xs), len(ys))
}
"#,
        )
        .is_empty(),
        "empty elided {{}} in []http.header and []url.values slots must pass"
    );

    assert_eq!(
        codes(
            r#"import "fmt"
import "net/http"
fn main() {
    xs := []http.header{{k: "v"}}
    fmt.println(len(xs))
}
"#,
        ),
        vec!["HYB-E1002"],
        "struct-field syntax {{k: v}} in []http.header slot must reject with HYB-E1002"
    );

    assert_eq!(
        codes(
            r#"import "fmt"
import "net/url"
fn main() {
    xs := []url.values{{k: "v"}}
    fmt.println(len(xs))
}
"#,
        ),
        vec!["HYB-E1002"],
        "struct-field syntax {{k: v}} in []url.values slot must reject with HYB-E1002"
    );
}

#[test]
fn rejects_duplicate_fields_on_elided_foreign_struct_literals() {
    assert_eq!(
        codes(
            r#"import "fmt"
import "sync"
fn main() {
    xs := []sync.mutex{{state: 1, state: 2}}
    fmt.println(len(xs))
}
"#,
        ),
        vec!["HYB-E1002"],
        "duplicate field names in elided foreign struct literal must reject with HYB-E1002"
    );
}
