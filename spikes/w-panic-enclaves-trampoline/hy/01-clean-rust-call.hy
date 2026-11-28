// EXPECTED-VERDICT: ACCEPT
// Normal Rust leaf call returning result with no panic.

fn main() {
    h := rust.open_enclave()
    res, err := h.call_safe()
    if err != nil {
        return
    }
}
