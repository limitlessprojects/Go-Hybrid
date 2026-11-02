// W-1427 Reproducer 01 — Class A: Causal Inversion / Stale Reads Inside the O8 Live Window
//
// Under Deferred Store-Back at Last Lexical Use (`last_use[n]`):
// - `n.bal = 99` mutates only the local copy `n`.
// - `read_bal(m)` and `m["a"].bal` observe stale `10` because `after_last_use_trigger := n.bal`
//   shifts `last_use[n]` past the map reads.
// - Adding or removing a pure read (`n.bal`) changes the result of earlier calls (`10` vs `99`).
//
// Under Write-Through + Exclusive A3 (#1427):
// - Emitter layer: `m["a"] = n` is emitted immediately after `n.bal = 99` (produces `99` in both cases).
// - Checker layer: Exclusive A3 rejects reading `m` (`read_bal(m)`, `m["a"].bal`) while `n` has
//   remaining uses (`HYB-E3102`).

struct acct { bal int }

fn read_bal(m map[string]acct) int {
    return m["a"].bal
}

fn main() {
    m := map[string]acct{"a": acct{bal: 10}}
    n := &m["a"]
    n.bal = 99
    during_loan := read_bal(m)
    direct_read := m["a"].bal
    after_last_use_trigger := n.bal
    after_loan := read_bal(m)
    fmt.println("n.bal=", after_last_use_trigger, "during_loan=", during_loan, "direct_read=", direct_read, "after_loan=", after_loan)
}
