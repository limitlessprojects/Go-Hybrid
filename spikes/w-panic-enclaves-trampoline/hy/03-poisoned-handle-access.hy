// EXPECTED-VERDICT: REJECT HYB-E3003
// Using a handle after it was poisoned by an enclave panic.

fn main() {
    h := rust.open_enclave()
    _, err := h.call_panicking_leaf()
    _ = err
    h.subsequent_call()
}
