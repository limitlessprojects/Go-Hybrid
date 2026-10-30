// EXPECTED-VERDICT: reject HYB-E3003
package samplepkg

fn bad_use_after_move() {
    let x = 42
    let y = move x
    let z = x
}
