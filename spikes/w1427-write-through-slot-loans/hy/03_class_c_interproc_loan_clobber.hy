// W-1427 Reproducer 03 — Class C: Interprocedural Slot-Loan Clobber on Pure Read
//
// Under Deferred Store-Back at Last Lexical Use:
// - `n := &m["a"]; n.bal = 99` buffers `99` in local `n`.
// - `set_five_hundred(m)` mutates `m["a"].bal = 500` via its own slot loan `p := &m["a"]`.
// - A subsequent pure read `last_read := n.bal` in `main` triggers the deferred store-back
//   `m["a"] = n`, silently overwriting `500` back to stale `99` (`final_bal= 99`).
//
// Under Write-Through + Exclusive A3 (#1427 + #1645/#2331):
// - Checker layer: `fn_mutates_param` / Exclusive A3 rejects calling `set_five_hundred(m)`
//   while `n` has remaining uses (`HYB-E3102`).
// - Emitter layer: `m["a"] = n` is emitted immediately after `n.bal = 99`, and pure read
//   `last_read := n.bal` never emits a store-back, preserving invariant `m[k] == *n` at every
//   statement boundary.

struct acct { bal int }

fn set_five_hundred(m map[string]acct) {
    p := &m["a"]
    p.bal = 500
}

fn main() {
    m := map[string]acct{"a": acct{bal: 10}}
    n := &m["a"]
    n.bal = 99
    set_five_hundred(m)
    after_call := m["a"].bal
    last_read := n.bal
    final_bal := m["a"].bal
    fmt.println("after_call=", after_call, "last_read=", last_read, "final_bal=", final_bal)
}
