//! handle_share_and_closure_const.rs — regression tests for:
//!   1. Boundary handle (`OwnHandle` / `BorrowHandle`) alias and `may_share`
//!      tracking across `:=` / `=`, helper calls, composite literals, field
//!      projections, own-handle field moves (`move s.doc`), and A2 borrow-loan
//!      conflicts (`HYB-E3101`, `HYB-E3003`, `HYB-E3004`).
//!   2. Same-statement double-moves and move-plus-read/write across `may_share`
//!      peers (`HYB-E3003`) plus chained move-defines of non-owned peers
//!      (`HYB-E3004`).
//!   3. `const` deep-immutability (`HYB-E3006`) inside closures across
//!      closure-local aliases/copies, closure-local `const` declarations,
//!      self-shadowing copies, pre-shadow writes, and block-scoped `const`s.

use hy_check::{check_file, CheckResult, Verdict};
use hy_syntax::parse_file;

fn check(src: &str) -> CheckResult {
    let ast = parse_file("test.hy", src).expect("test snippet must parse");
    check_file(&ast)
}

fn codes(r: &CheckResult) -> Vec<String> {
    r.diagnostics.iter().map(|d| d.code.clone()).collect()
}

// ---------------------------------------------------------------------------
// 1. Boundary handle alias / may-share tracking, own-handle field moves, A2
// ---------------------------------------------------------------------------

#[test]
fn own_handle_alias_use_after_consume_is_e3003_a7() {
    let r = check(
        "import \"rust/flate2\"\n\
         fn main() {\n\
             enc := flate2.new_writer(6)?\n\
             enc2 := enc\n\
             out := enc.finish()?\n\
             fmt.println(len(out))\n\
             enc2.write([]byte(\"trailer\"))?\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "got {:?}", r.diagnostics);
    assert_eq!(codes(&r), vec!["HYB-E3003"]);
    assert_eq!(r.diagnostics[0].tier0_rule.as_deref(), Some("A7"));
    assert!(
        r.diagnostics[0].notes.iter().any(|n| n.contains("O13")),
        "expected O13 alias-consume note, got {:?}",
        r.diagnostics[0].notes
    );
}

#[test]
fn own_handle_alias_and_call_alias_use_after_move_is_e3003() {
    let r1 = check(
        "import \"rust/serde_json\"\n\
         fn ship(d serde_json.value) {}\n\
         fn main() {\n\
             doc := serde_json.parse(`{}`)?\n\
             doc2 := doc\n\
             task ship(move doc)\n\
             doc2.set(\"k\", 1)?\n\
         }\n",
    );
    assert_eq!(r1.verdict, Verdict::Reject, "got {:?}", r1.diagnostics);
    assert!(codes(&r1).iter().any(|c| c == "HYB-E3003"));

    let r2 = check(
        "import \"rust/serde_json\"\n\
         fn same_doc(d serde_json.value) serde_json.value { return d }\n\
         fn ship(d serde_json.value) {}\n\
         fn main() {\n\
             doc := serde_json.parse(`{}`)?\n\
             doc2 := same_doc(doc)\n\
             task ship(move doc)\n\
             doc2.set(\"k\", 1)?\n\
         }\n",
    );
    assert_eq!(r2.verdict, Verdict::Reject, "got {:?}", r2.diagnostics);
    assert!(codes(&r2).iter().any(|c| c == "HYB-E3003"));
}

#[test]
fn own_handle_struct_peer_use_after_move_is_e3003() {
    let r = check(
        "import \"rust/serde_json\"\n\
         struct holder { doc serde_json.value }\n\
         fn ship(h holder) {}\n\
         fn main() {\n\
             doc := serde_json.parse(`{}`)?\n\
             h1 := holder{doc: doc}\n\
             h2 := h1\n\
             task ship(move h1)\n\
             h2.doc.set(\"k\", 1)?\n\
         }\n",
    );
    assert_eq!(r.verdict, Verdict::Reject, "got {:?}", r.diagnostics);
    assert!(codes(&r).iter().any(|c| c == "HYB-E3003"));
}

#[test]
fn own_handle_field_move_freezes_parent_and_rejects_param_move() {
    let r_use_after = check(
        "import \"rust/serde_json\"\n\
         struct holder { doc serde_json.value }\n\
         fn ship(d serde_json.value) {}\n\
         fn main() {\n\
             doc := serde_json.parse(`{}`)?\n\
             h := holder{doc: doc}\n\
             task ship(move h.doc)\n\
             h.doc.set(\"k\", 1)?\n\
         }\n",
    );
    assert_eq!(r_use_after.verdict, Verdict::Reject, "got {:?}", r_use_after.diagnostics);
    assert!(codes(&r_use_after).iter().any(|c| c == "HYB-E3003"));

    let r_local_move = check(
        "import \"rust/serde_json\"\n\
         struct holder { doc serde_json.value }\n\
         fn main() {\n\
             doc := serde_json.parse(`{}`)?\n\
             h := holder{doc: doc}\n\
             d := move h.doc\n\
             _ = d\n\
             doc.set(\"k\", 1)?\n\
         }\n",
    );
    assert_eq!(r_local_move.verdict, Verdict::Reject, "got {:?}", r_local_move.diagnostics);
    assert!(codes(&r_local_move).iter().any(|c| c == "HYB-E3003"));

    let r_param = check(
        "import \"rust/serde_json\"\n\
         struct holder { doc serde_json.value }\n\
         fn ship(d serde_json.value) {}\n\
         fn inspect(h holder) {\n\
             task ship(move h.doc)\n\
         }\n",
    );
    assert_eq!(r_param.verdict, Verdict::Reject, "got {:?}", r_param.diagnostics);
    assert!(codes(&r_param).iter().any(|c| c == "HYB-E3004"));
}

#[test]
fn borrow_view_used_after_handle_move_is_e3003_but_dead_view_accepts() {
    let r_reject = check(
        "import \"rust/serde_json\"\n\
         fn ship(d serde_json.value) {}\n\
         fn main() {\n\
             doc := serde_json.parse(`{}`)?\n\
             items := doc.array(\"items\")?\n\
             task ship(move doc)\n\
             fmt.println(items.len())\n\
         }\n",
    );
    assert_eq!(r_reject.verdict, Verdict::Reject, "got {:?}", r_reject.diagnostics);
    assert!(codes(&r_reject).iter().any(|c| c == "HYB-E3003"));

    let r_accept = check(
        "import \"rust/serde_json\"\n\
         fn ship(d serde_json.value) {}\n\
         fn main() {\n\
             doc := serde_json.parse(`{}`)?\n\
             items := doc.array(\"items\")?\n\
             fmt.println(items.len())\n\
             task ship(move doc)\n\
         }\n",
    );
    assert_eq!(r_accept.verdict, Verdict::Accept, "got {:?}", r_accept.diagnostics);
}

#[test]
fn a2_mutate_through_handle_alias_or_copied_or_rebound_view_is_e3101() {
    // Mutating through an alias of the borrowed handle while view is live.
    let r1 = check(
        "import \"rust/serde_json\"\n\
         fn main() {\n\
             doc := serde_json.parse(`{}`)?\n\
             doc2 := doc\n\
             items := doc.array(\"items\")?\n\
             doc2.set(\"k\", 1)?\n\
             fmt.println(items.len())\n\
         }\n",
    );
    assert_eq!(r1.verdict, Verdict::Reject, "got {:?}", r1.diagnostics);
    assert!(codes(&r1).iter().any(|c| c == "HYB-E3101"));

    // Copying a borrow view (`items2 := items`) extends the handle loan to `items2`'s last use.
    let r2 = check(
        "import \"rust/serde_json\"\n\
         fn main() {\n\
             doc := serde_json.parse(`{}`)?\n\
             items := doc.array(\"items\")?\n\
             items2 := items\n\
             doc.set(\"k\", 1)?\n\
             fmt.println(items2.len())\n\
         }\n",
    );
    assert_eq!(r2.verdict, Verdict::Reject, "got {:?}", r2.diagnostics);
    assert!(codes(&r2).iter().any(|c| c == "HYB-E3101"));

    // Rebinding a borrow view (`items = doc.array("b")?`) extends the new call loan to `items`'s last use.
    let r3 = check(
        "import \"rust/serde_json\"\n\
         fn main() {\n\
             doc := serde_json.parse(`{}`)?\n\
             items := doc.array(\"a\")?\n\
             items = doc.array(\"b\")?\n\
             doc.set(\"b\", 1)?\n\
             fmt.println(items.len())\n\
         }\n",
    );
    assert_eq!(r3.verdict, Verdict::Reject, "got {:?}", r3.diagnostics);
    assert!(codes(&r3).iter().any(|c| c == "HYB-E3101"));
}

// ---------------------------------------------------------------------------
// 2. Same-statement double-moves & move+read across may_share peers + O5.1
// ---------------------------------------------------------------------------

#[test]
fn same_stmt_double_move_and_move_plus_read_across_share_peers_is_e3003() {
    let r_struct = check(
        "struct holder { m map[string]int }\n\
         fn both(a holder, b holder) {}\n\
         fn main() {\n\
             h1 := holder{m: map[string]int{\"k\": 1}}\n\
             h2 := h1\n\
             task both(move h1, move h2)\n\
         }\n",
    );
    assert_eq!(r_struct.verdict, Verdict::Reject, "got {:?}", r_struct.diagnostics);
    assert!(codes(&r_struct).iter().any(|c| c == "HYB-E3003"));

    let r_slice = check(
        "fn both(a []int, b []int) {}\n\
         fn main() {\n\
             s1 := []int{1, 2, 3}\n\
             s2 := s1[0:2]\n\
             task both(move s1, move s2)\n\
         }\n",
    );
    assert_eq!(r_slice.verdict, Verdict::Reject, "got {:?}", r_slice.diagnostics);
    assert!(codes(&r_slice).iter().any(|c| c == "HYB-E3003"));

    let r_read_after = check(
        "struct holder { m map[string]int }\n\
         fn main() {\n\
             h1 := holder{m: map[string]int{\"k\": 1}}\n\
             h2 := h1\n\
             a, b := move h1, h2.m[\"k\"]\n\
             _ = a\n\
             _ = b\n\
         }\n",
    );
    assert_eq!(r_read_after.verdict, Verdict::Reject, "got {:?}", r_read_after.diagnostics);
    assert!(codes(&r_read_after).iter().any(|c| c == "HYB-E3003"));

    let r_read_before = check(
        "struct holder { m map[string]int }\n\
         fn main() {\n\
             h1 := holder{m: map[string]int{\"k\": 1}}\n\
             h2 := h1\n\
             a, b := h2.m[\"k\"], move h1\n\
             _ = a\n\
             _ = b\n\
         }\n",
    );
    assert_eq!(r_read_before.verdict, Verdict::Reject, "got {:?}", r_read_before.diagnostics);
    assert!(codes(&r_read_before).iter().any(|c| c == "HYB-E3003"));
}

#[test]
fn chained_move_define_and_literal_move_of_param_peer_is_e3004() {
    let r1 = check(
        "struct holder { m map[string]int }\n\
         fn worker(h holder) {}\n\
         fn leak(h holder) {\n\
             h2 := h\n\
             h3 := move h2\n\
             task worker(move h3)\n\
         }\n",
    );
    assert_eq!(r1.verdict, Verdict::Reject, "got {:?}", r1.diagnostics);
    assert!(codes(&r1).iter().any(|c| c == "HYB-E3004"));

    let r2 = check(
        "struct holder { m map[string]int }\n\
         fn worker(h holder) {}\n\
         fn leak(m map[string]int) {\n\
             box := holder{m: move m}\n\
             task worker(move box)\n\
         }\n",
    );
    assert_eq!(r2.verdict, Verdict::Reject, "got {:?}", r2.diagnostics);
    assert!(codes(&r2).iter().any(|c| c == "HYB-E3004"));
}

// ---------------------------------------------------------------------------
// 3. Closure const deep-immutability (HYB-E3006)
// ---------------------------------------------------------------------------

#[test]
fn closure_local_alias_and_copy_of_const_write_and_delete_is_e3006() {
    let r_alias = check(
        "const cells = [0]\n\
         fn main() {\n\
             f := fn() {\n\
                 alias := cells\n\
                 alias[0] = 1\n\
             }\n\
             f()\n\
         }\n",
    );
    assert_eq!(r_alias.verdict, Verdict::Reject, "got {:?}", r_alias.diagnostics);
    assert!(codes(&r_alias).iter().any(|c| c == "HYB-E3006"));

    let r_delete = check(
        "const tags = map[string]int{\"a\": 1}\n\
         fn main() {\n\
             f := fn() {\n\
                 alias := tags\n\
                 delete(alias, \"a\")\n\
             }\n\
             f()\n\
         }\n",
    );
    assert_eq!(r_delete.verdict, Verdict::Reject, "got {:?}", r_delete.diagnostics);
    assert!(codes(&r_delete).iter().any(|c| c == "HYB-E3006"));

    let r_rebind = check(
        "const cells = [0]\n\
         fn main() {\n\
             f := fn() {\n\
                 var alias []int = [9]\n\
                 alias = cells\n\
                 alias[0] = 1\n\
             }\n\
             f()\n\
         }\n",
    );
    assert_eq!(r_rebind.verdict, Verdict::Reject, "got {:?}", r_rebind.diagnostics);
    assert!(codes(&r_rebind).iter().any(|c| c == "HYB-E3006"));
}

#[test]
fn closure_local_const_and_enclosing_block_const_write_is_e3006() {
    let r_closure_const = check(
        "fn main() {\n\
             f := fn() {\n\
                 const local_c = [0]\n\
                 local_c[0] = 1\n\
             }\n\
             f()\n\
         }\n",
    );
    assert_eq!(r_closure_const.verdict, Verdict::Reject, "got {:?}", r_closure_const.diagnostics);
    assert!(codes(&r_closure_const).iter().any(|c| c == "HYB-E3006"));

    let r_block_const = check(
        "fn main() {\n\
             if true {\n\
                 const block_c = [0]\n\
                 f := fn() {\n\
                     block_c[0] = 1\n\
                 }\n\
                 f()\n\
             }\n\
         }\n",
    );
    assert_eq!(r_block_const.verdict, Verdict::Reject, "got {:?}", r_block_const.diagnostics);
    assert!(codes(&r_block_const).iter().any(|c| c == "HYB-E3006"));
}

#[test]
fn closure_pre_shadow_and_self_shadow_const_writes_are_e3006() {
    // Write to outer `cells` BEFORE shadowing `cells := [9]` later in the closure.
    let r_pre = check(
        "const cells = [0]\n\
         fn main() {\n\
             f := fn() {\n\
                 cells[0] = 1\n\
                 cells := [9]\n\
                 cells[0] = 2\n\
                 _ = cells\n\
             }\n\
             f()\n\
         }\n",
    );
    assert_eq!(r_pre.verdict, Verdict::Reject, "got {:?}", r_pre.diagnostics);
    assert_eq!(codes(&r_pre), vec!["HYB-E3006"]);

    // Write to outer `cells` AFTER an inner block-scoped shadow `cells := [9]` exits.
    let r_post_scope = check(
        "const cells = [0]\n\
         fn main() {\n\
             f := fn() {\n\
                 if true {\n\
                     cells := [9]\n\
                     cells[0] = 1\n\
                     _ = cells\n\
                 }\n\
                 cells[0] = 2\n\
             }\n\
             f()\n\
         }\n",
    );
    assert_eq!(r_post_scope.verdict, Verdict::Reject, "got {:?}", r_post_scope.diagnostics);
    assert_eq!(codes(&r_post_scope), vec!["HYB-E3006"]);

    // Self-shadowing copy `cells := cells` still points at `const cells` storage.
    let r_self_shadow = check(
        "const cells = [0]\n\
         fn main() {\n\
             cells := cells\n\
             cells[0] = 1\n\
         }\n",
    );
    assert_eq!(r_self_shadow.verdict, Verdict::Reject, "got {:?}", r_self_shadow.diagnostics);
    assert_eq!(codes(&r_self_shadow), vec!["HYB-E3006"]);
}
