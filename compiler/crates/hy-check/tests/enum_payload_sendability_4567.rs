//! enum_payload_sendability_4567.rs — regression tests for issue #4567:
//! a payload-carrying enum was classified `Primitive` unconditionally, so a
//! shared mutable payload (map/slice/...) inside an enum crossed spawn and
//! channel boundaries unproven while the same payload passed directly was
//! rejected with HYB-E3001. The fix classifies an enum by its
//! least-sendable payload (see `enum_shape_depth` in shape.rs): payload-less
//! enums stay `Primitive`; any non-sendable payload makes the whole enum
//! non-sendable; unresolvable payloads fail closed (`Unknown` → HYB-E3005).

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

fn assert_code(src: &str, want: &str) {
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.iter().any(|c| c == want),
        "expected {} , got {:?}",
        want,
        cs
    );
    assert_eq!(r.verdict, Verdict::Reject);
}

fn assert_accept(src: &str) {
    let r = check(src);
    let cs = codes(&r);
    assert!(
        cs.is_empty(),
        "expected accept (no error diagnostics), got {:?}",
        cs
    );
    assert_eq!(r.verdict, Verdict::Accept);
}

// ---------------------------------------------------------------------------
// Rejects — the shared payload must not launder through an enum
// ---------------------------------------------------------------------------

#[test]
fn issue_repro_enum_map_payload_is_e3001() {
    // Issue #4567's exact program: `get` returns the enum, the spawn takes
    // the shared map through it. The direct-map control rejects E3001; the
    // enum route must reject identically.
    assert_code(
        "enum box {\n    full(map[string]int)\n    empty\n}\n\nfn get(m map[string]int) box {\n    return box.full(m)\n}\n\nfn worker(b box, done chan<- bool) {\n    match b {\n        full(inner): {\n            for i in 0..2000 {\n                inner[\"k\"] = i\n            }\n        }\n        empty: {\n        }\n    }\n    done <- true\n}\n\nfn main() {\n    m := map[string]int{\"k\": 0}\n    done := make(chan bool, 1)\n    task worker(get(m), done)\n    for i in 0..2000 {\n        m[\"k\"] = i\n    }\n    <-done\n}\n",
        "HYB-E3001",
    );
}

#[test]
fn slice_payload_is_e3001() {
    // A slice payload is shared mutable storage exactly like a map payload.
    assert_code(
        "enum wrap {\n    items([]int)\n    none\n}\n\nfn get(xs []int) wrap {\n    return wrap.items(xs)\n}\n\nfn worker(w wrap, done chan<- bool) {\n    match w {\n        items(v): {\n            v[0] = 9\n        }\n        none: {\n        }\n    }\n    done <- true\n}\n\nfn main() {\n    xs := []int{1, 2}\n    done := make(chan bool, 1)\n    task worker(get(xs), done)\n    xs[0] = 7\n    <-done\n}\n",
        "HYB-E3001",
    );
}

#[test]
fn ascribed_binding_route_is_e3001() {
    // The ascription route: `var b box = get(m)` binds the enum, then the
    // binding itself crosses the spawn. Classification must follow the
    // binding's shape, not just the call-result shape.
    assert_code(
        "enum box {\n    full(map[string]int)\n    empty\n}\n\nfn get(m map[string]int) box {\n    return box.full(m)\n}\n\nfn worker(b box, done chan<- bool) {\n    match b {\n        full(inner): {\n            inner[\"k\"] = 1\n        }\n        empty: {\n        }\n    }\n    done <- true\n}\n\nfn main() {\n    m := map[string]int{\"k\": 0}\n    var b box = get(m)\n    done := make(chan bool, 1)\n    task worker(b, done)\n    m[\"k\"] = 2\n    <-done\n}\n",
        "HYB-E3001",
    );
}

#[test]
fn nested_enum_payload_is_e3001() {
    // Payload classification recurses: an outer enum carrying an inner enum
    // that carries a map is still a shared-map carrier.
    assert_code(
        "enum box {\n    full(map[string]int)\n    empty\n}\n\nenum outer {\n    has(box)\n    none\n}\n\nfn get(m map[string]int) outer {\n    return outer.has(box.full(m))\n}\n\nfn worker(o outer, done chan<- bool) {\n    match o {\n        has(b): {\n            match b {\n                full(inner): {\n                    inner[\"k\"] = 1\n                }\n                empty: {\n                }\n            }\n        }\n        none: {\n        }\n    }\n    done <- true\n}\n\nfn main() {\n    m := map[string]int{\"k\": 0}\n    done := make(chan bool, 1)\n    task worker(get(m), done)\n    m[\"k\"] = 2\n    <-done\n}\n",
        "HYB-E3001",
    );
}

#[test]
fn struct_payload_with_map_field_is_e3001() {
    // A struct payload is non-sendable at spawn boundaries already (bare
    // structs reject E3001); carrying one inside an enum must not change
    // that — the payload shape is what decides, and a map field makes it
    // shared mutable state either way.
    assert_code(
        "struct holder {\n    m map[string]int\n}\n\nenum box {\n    has(holder)\n    empty\n}\n\nfn get(h holder) box {\n    return box.has(h)\n}\n\nfn worker(b box, done chan<- bool) {\n    match b {\n        has(h): {\n            h.m[\"k\"] = 1\n        }\n        empty: {\n        }\n    }\n    done <- true\n}\n\nfn main() {\n    h := holder{m: map[string]int{\"k\": 0}}\n    done := make(chan bool, 1)\n    task worker(get(h), done)\n    <-done\n}\n",
        "HYB-E3001",
    );
}

#[test]
fn channel_send_of_map_enum_is_e3002() {
    // The same hole at a channel boundary: sending the enum must reject
    // with the channel-send code, not silently pass.
    assert_code(
        "enum box {\n    full(map[string]int)\n    empty\n}\n\nfn get(m map[string]int) box {\n    return box.full(m)\n}\n\nfn main() {\n    m := map[string]int{\"k\": 0}\n    ch := make(chan box, 1)\n    ch <- get(m)\n}\n",
        "HYB-E3002",
    );
}

#[test]
fn respawn_of_received_enum_param_is_e3001() {
    // A box received as a parameter keeps its payload shape: re-spawning
    // it into a second task must reject, not launder through the param.
    assert_code(
        "enum box {\n    full(map[string]int)\n    empty\n}\n\nfn inner(b box, done chan<- bool) {\n    match b {\n        full(inner): {\n            inner[\"k\"] = 1\n        }\n        empty: {\n        }\n    }\n    done <- true\n}\n\nfn worker(b box, done chan<- bool) {\n    done2 := make(chan bool, 1)\n    task inner(b, done2)\n    <-done2\n    done <- true\n}\n\nfn main() {\n    m := map[string]int{\"k\": 0}\n    done := make(chan bool, 1)\n    task worker(box.full(m), done)\n    m[\"k\"] = 2\n    <-done\n}\n",
        "HYB-E3001",
    );
}

// ---------------------------------------------------------------------------
// Accepts — payload-less and sendable-payload enums keep crossing
// ---------------------------------------------------------------------------

#[test]
fn int_payload_enum_accepts() {
    assert_accept(
        "enum tag {\n    num(int)\n    none\n}\n\nfn get() tag {\n    return tag.num(7)\n}\n\nfn worker(t tag, done chan<- bool) {\n    match t {\n        num(n): {\n            fmt.println(\"{n}\")\n        }\n        none: {\n        }\n    }\n    done <- true\n}\n\nfn main() {\n    done := make(chan bool, 1)\n    task worker(get(), done)\n    <-done\n}\n",
    );
}

#[test]
fn string_payload_enum_accepts() {
    assert_accept(
        "enum msg {\n    text(string)\n    none\n}\n\nfn get() msg {\n    return msg.text(\"hi\")\n}\n\nfn worker(t msg, done chan<- bool) {\n    match t {\n        text(s): {\n            fmt.println(\"{s}\")\n        }\n        none: {\n        }\n    }\n    done <- true\n}\n\nfn main() {\n    done := make(chan bool, 1)\n    task worker(get(), done)\n    <-done\n}\n",
    );
}

#[test]
fn payloadless_enum_accepts() {
    assert_accept(
        "enum color {\n    red\n    green\n    blue\n}\n\nfn get() color {\n    return color.red\n}\n\nfn worker(c color, done chan<- bool) {\n    match c {\n        red: {\n            fmt.println(\"r\")\n        }\n        green: {\n        }\n        blue: {\n        }\n    }\n    done <- true\n}\n\nfn main() {\n    done := make(chan bool, 1)\n    task worker(get(), done)\n    <-done\n}\n",
    );
}

#[test]
fn chan_payload_enum_accepts() {
    // Channels are sendable by type, so an enum carrying one stays
    // sendable — the payload rule must not over-reject. (The worker only
    // matches on the variant and never receives from the arm-bound
    // channel: match-arm bindings for enum payloads do not yet carry the
    // payload's shape — a separate pre-existing limitation, out of scope
    // for #4567 — so the test asserts the boundary, not the arm use.)
    assert_accept(
        "enum sig {\n    send(chan bool)\n    none\n}\n\nfn get(c chan bool) sig {\n    return sig.send(c)\n}\n\nfn worker(s sig, done chan<- bool) {\n    match s {\n        send(c): {\n            fmt.println(\"got\")\n        }\n        none: {\n        }\n    }\n    done <- true\n}\n\nfn main() {\n    c := make(chan bool, 1)\n    done := make(chan bool, 1)\n    task worker(get(c), done)\n    c <- true\n    <-done\n}\n",
    );
}

#[test]
fn map_enum_used_locally_only_accepts() {
    // No boundary crossed, no diagnostic: the enum is only used inside
    // main, so the payload shape must not reject local use.
    assert_accept(
        "enum box {\n    full(map[string]int)\n    empty\n}\n\nfn main() {\n    m := map[string]int{\"k\": 0}\n    b := box.full(m)\n    match b {\n        full(inner): {\n            inner[\"k\"] = 1\n        }\n        empty: {\n        }\n    }\n}\n",
    );
}

#[test]
fn move_of_map_enum_accepts() {
    // `move` is the sanctioned ownership-transfer route: moving a whole
    // LOCAL enum binding (built from a fresh map, with no retained alias)
    // across the spawn must stay accepted, exactly as moving a bare map
    // binding does. (`move` applies to whole bindings only — `move get()`
    // on a call result is E3004 by the pre-existing O5.4 rule, and moving
    // a PARAMETER is E3004 too since the caller retains it, for enums and
    // bare maps alike — so this test moves a local binding, not a call
    // result or a parameter.)
    assert_accept(
        "enum box {\n    full(map[string]int)\n    empty\n}\n\nfn get() box {\n    return box.full(map[string]int{\"k\": 0})\n}\n\nfn worker(b box, done chan<- bool) {\n    match b {\n        full(inner): {\n            inner[\"k\"] = 1\n        }\n        empty: {\n        }\n    }\n    done <- true\n}\n\nfn main() {\n    var b box = get()\n    done := make(chan bool, 1)\n    task worker(move b, done)\n    <-done\n}\n",
    );
}
