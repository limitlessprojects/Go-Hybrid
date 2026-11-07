// W-3884 (Issue #3884) sample 01: ACCEPT-MEMBRANE — Zero-Copy Region Capability Witnesses
// Lowered to pure-Go `BufferMembrane[R]` with compile-time zero-sized capability token `RegionCap[R]`.
// Eliminates foreign function buffer copies, CGO trampolines, and whole-program loan graphs.
package main

import (
    "fmt"
    "hybrid/membrane"
)

fn process_telemetry_batch[R membrane.Region](cap membrane.RegionCap[R], m membrane.BufferMembrane[R]) error:
    data := m.view(cap)
    fmt.Printf("Received %d bytes via zero-copy membrane\n", len(data))
    return nil

fn main():
    // Allocate 100MB buffer once on heap
    large_payload := make([]byte, 100 * 1024 * 1024)

    // Execute within RegionA boundary
    membrane.enter_region[membrane.RegionA](fn(capA membrane.RegionCap[membrane.RegionA]):
        mA, err := membrane.new_buffer_membrane(capA, large_payload)
        if err != nil:
            return

        // Zero-copy transfer across boundary to RegionB
        mB, capB, err := membrane.transfer_buffer[membrane.RegionA, membrane.RegionB](mA, &capA)
        if err != nil:
            return

        process_telemetry_batch[membrane.RegionB](capB, mB)
    )
