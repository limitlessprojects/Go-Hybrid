// EXPECTED-VERDICT: REJECT HYB-E3801
// Foreign unmanaged Rust unwinding without trampoline enclave boundary.

fn main() {
    foreign.raw_unwind_call()
}
