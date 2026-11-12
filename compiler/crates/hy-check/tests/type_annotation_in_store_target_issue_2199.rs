use hy_check::check_file;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("test.hy", src).expect("parses");
    check_file(&ast)
        .diagnostics
        .into_iter()
        .map(|d| d.code)
        .collect()
}

fn assert_has_code(src: &str, want: &str) {
    let cs = codes(src);
    assert!(
        cs.iter().any(|c| c == want),
        "expected [{want}] in {cs:?} for:\n{src}"
    );
}

fn assert_clean(src: &str) {
    let cs = codes(src);
    assert!(
        cs.is_empty(),
        "expected 0 diagnostics, got {cs:?} for:\n{src}"
    );
}

#[test]
fn typed_store_targets_reject_e1002_issue_2199() {
    // 1. local binding with wrong type annotation
    let src1 = r#"
import "fmt"

fn main() {
    var n int
    n string = "str"
    fmt.println("{n}")
}
"#;
    assert_has_code(src1, "HYB-E1002");

    // 2. local binding with matching type annotation
    let src2 = r#"
import "fmt"

fn main() {
    var n int
    n int = 5
    fmt.println("{n}")
}
"#;
    assert_has_code(src2, "HYB-E1002");

    // 3. struct field with type annotation
    let src3 = r#"
import "fmt"

struct q_holder {
    a int
}

fn main() {
    var q q_holder
    q.a int = 5
    fmt.println("{q.a}")
}
"#;
    assert_has_code(src3, "HYB-E1002");

    // 4. bare type name as assignment target
    let src4 = r#"
fn main() {
    int = 5
}
"#;
    assert_has_code(src4, "HYB-E1002");

    let src5 = r#"
struct my_rec {
    v int
}

fn main() {
    my_rec = 1
}
"#;
    assert_has_code(src5, "HYB-E1002");
}

#[test]
fn positive_controls_stay_accepted_issue_2199() {
    // Plain variable store
    let src1 = r#"
import "fmt"

fn main() {
    var n int
    n = 5
    fmt.println("{n}")
}
"#;
    assert_clean(src1);

    // Plain struct field store
    let src2 = r#"
import "fmt"

struct q_holder {
    a int
}

fn main() {
    var q q_holder
    q.a = 5
    fmt.println("{q.a}")
}
"#;
    assert_clean(src2);

    // Var declaration with explicit type
    let src3 = r#"
import "fmt"

fn main() {
    var n int = 5
    fmt.println("{n}")
}
"#;
    assert_clean(src3);

    // Short declaration
    let src4 = r#"
import "fmt"

fn main() {
    n := 5
    fmt.println("{n}")
}
"#;
    assert_clean(src4);
}
