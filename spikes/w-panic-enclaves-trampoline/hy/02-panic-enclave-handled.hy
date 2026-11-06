// EXPECTED-VERDICT: ACCEPT
// Panicking Rust leaf contained by enclave, safely handled via error check.

fn main() {
    h := rust.open_enclave()
    res, err := h.call_panicking_leaf()
    if errors.is(err, hy.err_panic_enclave) {
        fmt.println("handled cleanly")
    }
}
