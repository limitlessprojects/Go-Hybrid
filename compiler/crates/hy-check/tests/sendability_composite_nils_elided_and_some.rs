//! Regression tests for `sendability.rs` (`check_composite_nils`) enforcing
//! SPEC N1 null-safety (`HYB-E4001`, GUARANTEES Row 6 & Row 7) on:
//!   1. Type-elided struct literals (`[]holder{{m: nil}}`, `[]*holder{{m: nil}}`,
//!      `map[string]holder{"a": {m: nil}}`).
//!   2. Type-elided slice/array literals inside map values (`map[string][]*int{"a": [nil]}`).
//!   3. Map literal keys (`map[*int]int{nil: 1}`).
//!   4. `some(nil)` option constructors (`var o option[*int] = some(nil)`).

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
fn rejects_nil_in_elided_struct_and_slice_composite_slots() {
    assert_eq!(
        codes(
            r#"import "fmt"
struct holder {
    m map[string]int
}
fn main() {
    xs := []holder{{m: nil}}
    ys := []*holder{{m: nil}}
    m := map[string]holder{"a": {m: nil}}
    fmt.println(len(xs), len(ys), len(m))
}
"#,
        ),
        vec!["HYB-E4001", "HYB-E4001", "HYB-E4001"],
        "nil map fields inside elided struct literals in slice/ptr-slice/map slots must reject with HYB-E4001"
    );

    assert_eq!(
        codes(
            r#"import "fmt"
fn main() {
    m := map[string][]*int{"a": [nil]}
    fmt.println(len(m))
}
"#,
        ),
        vec!["HYB-E4001"],
        "nil element inside elided slice literal in map value slot must reject with HYB-E4001"
    );
}

#[test]
fn rejects_nil_in_map_keys_and_some_constructor() {
    assert_eq!(
        codes(
            r#"import "fmt"
fn main() {
    m := map[*int]int{nil: 1}
    fmt.println(len(m))
}
"#,
        ),
        vec!["HYB-E4001"],
        "nil map key on non-nilable pointer type `*int` must reject with HYB-E4001"
    );

    assert_eq!(
        codes(
            r#"import "fmt"
fn main() {
    var o option[*int] = some(nil)
    fmt.println(o.is_some())
}
"#,
        ),
        vec!["HYB-E4001"],
        "`some(nil)` must reject with HYB-E4001"
    );
}
