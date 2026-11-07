// EXPECTED-VERDICT: REJECT HYB-E3003
// Dereferencing an OwnHandle whose generation tag is marked POISONED.

fn main() {
    h := rust.open_enclave()
    _ = h.mutate_and_panic()
    x := *h
}
