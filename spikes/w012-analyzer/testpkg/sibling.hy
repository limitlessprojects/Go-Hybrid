// EXPECTED-VERDICT: reject HYB-E3003
package testpkg

fn bad_borrow() {
    let x = 42
    let y = move x
    let z = x
}
