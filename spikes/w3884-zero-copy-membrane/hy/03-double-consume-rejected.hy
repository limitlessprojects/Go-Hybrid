// W-3884 (Issue #3884) sample 03: REJECT (HYB-E3884-USE-AFTER-MOVE)
// Attempting to reuse a capability witness after it has been consumed by `transfer_buffer`
// is rejected statically at compile time (and trapped by monotonic epoch invalidation).
package main

import "hybrid/membrane"

fn main():
    buf := make([]byte, 1024)
    membrane.enter_region[membrane.RegionA](fn(capA membrane.RegionCap[membrane.RegionA]):
        mA, _ := membrane.new_buffer_membrane(capA, buf)
        mB, capB, _ := membrane.transfer_buffer[membrane.RegionA, membrane.RegionB](mA, &capA)

        // Illegal: capA was consumed by transfer_buffer
        _ = mA.view(capA) // ERROR HYB-E3884-USE-AFTER-MOVE: capA has been consumed
        _ = capB
        _ = mB
    )
