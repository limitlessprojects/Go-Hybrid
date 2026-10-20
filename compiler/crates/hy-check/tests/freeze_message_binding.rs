//! freeze_message_binding.rs — the HYB-E3003 note must name the binding whose
//! ownership actually left.
//!
//! Before the fix the note was built from `resolve_alias(used_binding)`. That is
//! the right name when the use reaches the moved value through the
//! single-parent `aliases` chain, but a **may-share peer** (#27) is not in that
//! chain: reading a reference-typed FIELD or ELEMENT into a local records a
//! may-share edge, so `resolve_alias` on the local returned the local itself.
//! The result was a note that contradicted the program text:
//!
//! ```text
//! = ownership of `d` left this task at the spawn `task worker(...)`
//! = `d` aliases `d` — the freeze covers every live alias ...
//! ```
//!
//! for `task worker(move h, done)` — a move of `h`, reported as a move of `d`
//! that aliases itself. `MoveInfo` now carries the moved binding's name, and the
//! note uses it, so the diagnostic names the transfer that actually happened.
//! These tests pin the corrected wording on the may-share path AND on the plain
//! alias path, so a future change cannot fix one by breaking the other.

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

/// The single HYB-E3003 diagnostic, with its notes joined for text assertions.
fn e3003_notes(r: &hy_check::CheckResult) -> String {
    let errs: Vec<_> = r
        .diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .collect();
    assert_eq!(
        errs.len(),
        1,
        "expected exactly one error, got {:?}",
        errs.iter().map(|d| d.code.clone()).collect::<Vec<_>>()
    );
    let d = errs[0];
    assert_eq!(d.code, "HYB-E3003", "{}", d.message);
    assert_eq!(d.tier0_rule.as_deref(), Some("A1b"), "{}", d.message);
    d.notes.join("\n")
}

// ---------------------------------------------------------------------------
// The may-share path: a reference-typed FIELD read out before the move
// ---------------------------------------------------------------------------

#[test]
fn field_read_alias_note_names_the_moved_binding() {
    let r = check(
        "struct holder {\n\
         \x20   data []int\n\
         }\n\
         \n\
         fn worker(h holder, done chan<- bool) {\n\
         \x20   fmt.println(\"first: {h.data[0]}\")\n\
         \x20   done <- true\n\
         }\n\
         \n\
         fn main() {\n\
         \x20   h := holder{data: []int{1, 2, 3}}\n\
         \x20   d := h.data\n\
         \x20   done := make(chan bool, 1)\n\
         \x20   task worker(move h, done)\n\
         \x20   d[0] = 9\n\
         \x20   <-done\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject);

    let notes = e3003_notes(&r);
    assert!(
        notes.contains("ownership of `h` left this task"),
        "the note must name the moved binding `h`, got:\n{notes}"
    );
    assert!(
        notes.contains("`d` aliases `h`"),
        "the note must say `d` aliases `h`, got:\n{notes}"
    );
    // The pre-fix wording: the moved name came from the used binding's alias
    // root, which is the peer itself on the may-share path.
    assert!(
        !notes.contains("ownership of `d` left this task"),
        "the note must not claim `d` was moved, got:\n{notes}"
    );
    assert!(
        !notes.contains("`d` aliases `d`"),
        "a binding cannot alias itself, got:\n{notes}"
    );
}

// ---------------------------------------------------------------------------
// The plain alias path: the note already named the origin, and still must
// ---------------------------------------------------------------------------

#[test]
fn map_element_alias_note_still_names_the_origin() {
    let r = check(
        "fn worker(m map[string]map[string]int, done chan<- bool) {\n\
         \x20   fmt.println(\"r: {m[\"a\"][\"b\"]}\")\n\
         \x20   done <- true\n\
         }\n\
         \n\
         fn main() {\n\
         \x20   m := map[string]map[string]int{\"a\": map[string]int{\"b\": 1}}\n\
         \x20   inner := m[\"a\"]\n\
         \x20   done := make(chan bool, 1)\n\
         \x20   task worker(move m, done)\n\
         \x20   inner[\"b\"] = 9\n\
         \x20   <-done\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject);

    let notes = e3003_notes(&r);
    assert!(
        notes.contains("ownership of `m` left this task"),
        "got:\n{notes}"
    );
    assert!(notes.contains("`inner` aliases `m`"), "got:\n{notes}");
}
