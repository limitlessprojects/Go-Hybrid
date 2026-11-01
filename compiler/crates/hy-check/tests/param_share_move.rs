//! param_share_move.rs — regression tests for issue #75: moving a MAY-SHARE
//! PEER of a parameter escaped the O5.1 non-owned check.
//!
//! `check_non_owned_move` used to derive `non_owned_origin` from the
//! binding's own origin or its `aliases`-chain root only. The may-share
//! graph (#27/#35 — undirected, covering shallow struct/array copies,
//! re-slices, projections, and composite captures) was not consulted, and
//! `record_alias`'s define-time origin inheritance only fires for
//! reference-alias shapes (Map/Slice/Pointer). So `m2 := m` (map param)
//! was correctly rejected via the alias+origin path, but `h2 := h`
//! (shallow struct copy of a struct param carrying a map) and `a2 := a`
//! (shallow fixed-array copy) recorded a share edge and NO origin: the
//! `move` passed with 0 diagnostics while the emitted Go raced
//! (`fatal error: concurrent map writes`). The fix scans the binding's
//! may-share component (and each peer's alias root) for a non-owned
//! origin at move time. Corpus pins: 29 (struct peer, send/spawn), 30
//! (array peer).

use hy_check::{check_file, Verdict};
use hy_diag::Severity;
use hy_syntax::parse_file;

fn check(src: &str) -> hy_check::CheckResult {
    let path = "inline.hy";
    let ast = parse_file(path, src)
        .unwrap_or_else(|diags| panic!("must parse (parser contract): {:?}", diags));
    check_file(&ast)
}

fn codes(r: &hy_check::CheckResult) -> Vec<String> {
    r.diagnostics
        .iter()
        .filter(|d| d.severity == Severity::Error)
        .map(|d| d.code.clone())
        .collect()
}

const STRUCT_PEER_OF_PARAM: &str = "\
struct holder {
    m map[string]int
}

fn worker(h holder, done chan<- bool) {
    for i in 0..10000 {
        h.m[\"k\"] = i
    }
    done <- true
}

fn outer(h holder, done chan<- bool) {
    h2 := h
    task worker(move h2, done)
}

fn main() {
    h := holder{m: map[string]int{\"k\": 0}}
    done := make(chan bool, 1)
    outer(h, done)
    <-done
}
";

#[test]
fn struct_peer_of_param_move_is_rejected_e3004() {
    // The issue's repro: `h2 := h` is a shallow struct copy (may-share
    // peer, NOT a reference alias), so `move h2` used to pass while the
    // caller kept a live binding to the same storage — `go run` hit
    // `fatal error: concurrent map writes`. O5.1 applies identically to
    // the peer: the storage is not exclusively owned by the sending frame.
    let r = check(STRUCT_PEER_OF_PARAM);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3004"),
        "expected HYB-E3004 for moving the struct share peer of a param, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
    let e3004 = r
        .diagnostics
        .iter()
        .find(|d| d.code == "HYB-E3004")
        .expect("E3004 present");
    assert!(
        e3004.message.contains("may-share peer"),
        "the peer-path diagnostic must name the share relation, got: {}",
        e3004.message
    );
}

const STRUCT_PEER_OF_PARAM_SEND: &str = "\
struct holder {
    m map[string]int
}

fn outer(h holder, ch chan<- holder) {
    h2 := h
    ch <- move h2
}

fn main() {
    h := holder{m: map[string]int{\"k\": 0}}
    ch := make(chan holder, 1)
    outer(h, ch)
}
";

#[test]
fn struct_peer_of_param_send_is_rejected_e3004() {
    // The send form of the same gap (check_non_owned_move is shared by
    // the spawn and send paths).
    let r = check(STRUCT_PEER_OF_PARAM_SEND);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3004"),
        "expected HYB-E3004 for the send-form peer move, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const ARRAY_PEER_OF_PARAM: &str = "\
fn worker(a [2]map[string]int, done chan<- bool) {
    for i in 0..10000 {
        a[0][\"k\"] = i
    }
    done <- true
}

fn outer(a [2]map[string]int, done chan<- bool) {
    a2 := a
    task worker(move a2, done)
}

fn main() {
    var a [2]map[string]int
    a[0] = map[string]int{\"k\": 0}
    done := make(chan bool, 1)
    outer(a, done)
    <-done
}
";

#[test]
fn array_peer_of_param_move_is_rejected_e3004() {
    // The array twin: a fixed array is not a reference alias, so the
    // alias/origin-inheritance path skips it — only the share-group scan
    // can see the peer-of-param relation.
    let r = check(ARRAY_PEER_OF_PARAM);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3004"),
        "expected HYB-E3004 for moving the array share peer of a param, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const MAP_PEER_OF_PARAM: &str = "\
fn worker(m map[string]int, done chan<- bool) {
    for i in 0..10000 {
        m[\"k\"] = i
    }
    done <- true
}

fn outer(m map[string]int, done chan<- bool) {
    m2 := m
    task worker(move m2, done)
}

fn main() {
    m := map[string]int{\"k\": 0}
    done := make(chan bool, 1)
    outer(m, done)
    <-done
}
";

#[test]
fn map_peer_of_param_still_rejected_e3004() {
    // Regression control: the map twin was already rejected through the
    // alias + define-time origin-inheritance path — the share-group scan
    // must not weaken or re-route it (message keeps the non-share
    // "not parameter `m`" spelling).
    let r = check(MAP_PEER_OF_PARAM);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == "HYB-E3004"),
        "expected HYB-E3004 for the map twin (regression), got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
    let e3004 = r
        .diagnostics
        .iter()
        .find(|d| d.code == "HYB-E3004")
        .expect("E3004 present");
    assert!(
        !e3004.message.contains("may-share peer"),
        "the alias-path diagnostic must keep its original spelling, got: {}",
        e3004.message
    );
}

const PURE_LOCAL_PEER: &str = "\
struct holder {
    m map[string]int
}

fn worker(h holder, done chan<- bool) {
    for i in 0..10000 {
        h.m[\"k\"] = i
    }
    done <- true
}

fn main() {
    h1 := holder{m: map[string]int{\"k\": 0}}
    h2 := h1
    done := make(chan bool, 1)
    task worker(move h1, done)
    for i in 0..10000 {
        h2.m[\"k\"] = i
    }
    <-done
}
";

#[test]
fn pure_local_peer_move_stays_accepted_by_o51() {
    // Precision control: `h1` is an exclusively owned LOCAL (not a
    // param/capture/range var). `h2 := h1` may-shares with it, but no
    // peer in the component carries a non-owned origin — so O5.1 must
    // NOT reject the move. (The post-move write through `h2` is a
    // separate O5.2 freeze violation, HYB-E3003, pinned by corpus 20 /
    // call_share.rs's lexical twin — here we only pin that E3004 does
    // not fire.)
    let r = check(PURE_LOCAL_PEER);
    let cs = codes(&r);
    assert!(
        !cs.iter().any(|c| c == "HYB-E3004"),
        "a pure-local share peer must not trip the O5.1 non-owned check, got {:?}",
        cs
    );
    assert!(
        cs.iter().any(|c| c == "HYB-E3003"),
        "expected the O5.2 freeze (E3003) on the post-move peer write, got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

const PURE_LOCAL_MOVE_CLEAN: &str = "\
struct holder {
    m map[string]int
}

fn worker(h holder, done chan<- bool) {
    for i in 0..10000 {
        h.m[\"k\"] = i
    }
    done <- true
}

fn main() {
    h1 := holder{m: map[string]int{\"k\": 0}}
    h2 := h1
    done := make(chan bool, 1)
    pre := h2.m[\"k\"]
    fmt.println(pre)
    task worker(move h1, done)
    <-done
}
";

#[test]
fn pure_local_define_without_post_move_use_stays_accept() {
    // Full accept control: the same pure-local peer with NO post-move
    // use of the peer — the whole program stays clean (no E3004, no
    // E3003). Guards against the share-group scan over-reaching into
    // owned-local components.
    let r = check(PURE_LOCAL_MOVE_CLEAN);
    assert!(
        codes(&r).is_empty(),
        "owned-local peer component must stay clean, got {:?}",
        codes(&r)
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

#[test]
fn e3004_fixit_notes_advise_deep_copy_issue_2272() {
    let src = "
fn worker(m map[string]int) {
    m[\"x\"] = 1
}

fn spawner(m map[string]int) {
    task worker(move m)
}
";
    let r = check(src);
    assert_eq!(r.verdict, Verdict::Reject);
    let e3004 = r
        .diagnostics
        .iter()
        .find(|d| d.code == "HYB-E3004")
        .expect("E3004 present");
    let fix_note = e3004
        .notes
        .iter()
        .find(|n| n.starts_with("fix:"))
        .expect("fix note present");
    assert!(
        fix_note.contains("deep-copy"),
        "fix-it must advise a deep copy, got: {}",
        fix_note
    );
    assert!(
        !fix_note.contains("`fresh := ...; move fresh`"),
        "fix-it must not advise shallow fresh := ... rebinding, got: {}",
        fix_note
    );
}
