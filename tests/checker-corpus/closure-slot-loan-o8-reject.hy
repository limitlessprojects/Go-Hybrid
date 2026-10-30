// ============================================================================
// closure-slot-loan-o8-reject — Checker corpus (soundness issue #1403,
// reported against main 1f0ff97).
//
// PURPOSE: pin O8 slot-loan uniqueness and A3 alias tracking INSIDE a closure
// body. `check_closure_slot_loans` (closures.rs) pushes `LoanTarget::MapSlot`
// entries for `p := &m[k]` / `p = &m[k]` without first calling
// `emit_o8_alias_if_live`, never checks `walk::place_field_store_slot` on an
// assignment target, and does not record closure-local alias declarations
// (`alias := m`) in `st.aliases`. The same three shapes REJECT at top level
// (corpus 08 / 42, `slot_loan_unique.rs`), but inside a closure body they
// passed with 0 diagnostics on main.
//
// ---- Loans traced (non-lexical windows) ----
//   L1 = `p := &m["a"]` — UNIQUE map-slot loan; window creation → last use
//        (`p.bal = p.bal + 1`). The second loan `q := &m["a"]` falls inside
//        the window → O8 VIOLATION (HYB-E3102).
//   L2 = `s := &ns[0]` — UNIQUE slot loan; window creation → last use
//        (`s.bal = s.bal + 1`). The field store `ns[0].bal = 50` falls inside
//        the window → O8 VIOLATION (HYB-E3102).
//   L3 = `r := &m["a"]` — UNIQUE slot loan through `alias` (a shallow alias of
//        `m`); the insert `alias["c"] = ...` falls inside the window
//        → A3 VIOLATION (HYB-E3102).
//
// ---- Expected verdict ----
// EXPECTED-VERDICT: REJECT A3
//   REJECT — O8 uniqueness and A3 invalidation must hold inside closure bodies
//   exactly as they do at top level; the emitter lowers each slot loan to
//   copy-out / store-back, so these accesses silently clobber each other.
// ============================================================================

struct acct {
    bal int
}

fn main() {
    m := map[string]acct{"a": acct{bal: 1}, "b": acct{bal: 2}}
    ns := []acct{acct{bal: 1}}
    f := fn() {
        // 1. O8 duplicate live slot loan on m["a"]
        p := &m["a"]
        q := &m["a"]
        q.bal = 50
        p.bal = p.bal + 1

        // 2. O8 field store through base while slot loan is live
        s := &ns[0]
        ns[0].bal = 50
        s.bal = s.bal + 1

        // 3. A3 map insert through closure-local alias while slot loan is live
        alias := m
        r := &m["a"]
        alias["c"] = acct{bal: 9}
        r.bal = 99
    }
    f()
}
