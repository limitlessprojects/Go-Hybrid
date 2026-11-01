//! scalar_const_copy_write_false_positive.rs — false-positive regression for
//! HYB-E3006.
//!
//! `x = k` records the RHS binding's `non_owned_origin` on the target. For a
//! file-level `const k`, that origin is "file-level \`const k\`", and
//! `binding_const_root` resolves the name out of it. If `k` is a scalar the
//! copy is a plain value copy — Go lets you rebind `x` freely — but before
//! this fix every later write to `x` was reported as a write to the const:
//!
//!     const k = 3
//!     fn main() {
//!         var x float64
//!         x = k
//!         x = 1          // HYB-E3006: cannot write to `k`   (false positive)
//!     }
//!
//! The write-through rule (#123) only applies when the copy can share the
//! const's backing storage: maps, slices, pointers, handles, and
//! structs/arrays containing them. Those rejects are pinned below so the fix
//! does not weaken T22.

use hy_check::check_file;
use hy_diag::Severity;
use hy_syntax::parse_file;

fn codes(src: &str) -> Vec<String> {
    let ast = parse_file("inline.hy", src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

fn assert_accept(src: &str) {
    let found = codes(src);
    assert!(
        found.is_empty(),
        "expected acceptance, got {found:?} for:\n{src}"
    );
}

fn assert_reject(src: &str) {
    let found = codes(src);
    assert!(
        found.iter().any(|code| code == "HYB-E3006"),
        "expected HYB-E3006, got {found:?} for:\n{src}"
    );
}

// ---- scalar const copies stay writable ----

#[test]
fn scalar_const_copy_assign_stays_writable() {
    assert_accept(
        r#"const k = 3
fn main() {
    var x float64
    x = k
    x = 1
    x += 1
    fmt.println("{x}")
}
"#,
    );
}

#[test]
fn scalar_const_copy_paren_stays_writable() {
    assert_accept(
        r#"const k = 3
fn main() {
    var x float64
    x = (k)
    x = -k
    fmt.println("{x}")
}
"#,
    );
}

#[test]
fn scalar_const_copy_unary_stays_writable() {
    assert_accept(
        r#"const k = 3
fn main() {
    var x float64 = k
    x = (k)
    x = -k
    fmt.println("{x}")
}
"#,
    );
}

#[test]
fn string_const_copy_stays_writable() {
    assert_accept(
        r#"const s = "a"
fn main() {
    var t string
    t = s
    t = "b"
    fmt.println(t)
}
"#,
    );
}

#[test]
fn local_scalar_const_copy_stays_writable() {
    // int target: a local untyped const is not tracked by the buildable
    // Float/Int untyped-constant set, which is a separate (fail-open) rule.
    assert_accept(
        r#"fn main() {
    const k = 3
    var y int
    y = k
    y = 1
    fmt.println(y)
}
"#,
    );
}

// ---- reference-shaped const copies still reject (T22 / #123) ----

#[test]
fn map_const_copy_still_rejects() {
    assert_reject(
        r#"const m = map[string]int{"a": 1}
fn main() {
    var y map[string]int
    y = m
    y["b"] = 2
    fmt.println(y)
}
"#,
    );
}

#[test]
fn slice_const_copy_still_rejects() {
    assert_reject(
        r#"const xs = []int{1}
fn main() {
    var ys []int
    ys = xs
    ys[0] = 2
    fmt.println(ys)
}
"#,
    );
}

#[test]
fn local_map_const_copy_still_rejects() {
    assert_reject(
        r#"fn main() {
    const m = map[string]int{"a": 1}
    var y map[string]int
    y = m
    y["b"] = 2
    fmt.println(y)
}
"#,
    );
}
