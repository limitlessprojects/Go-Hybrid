// W-4699 (Issue #4699) sample 02: REJECT — A worker that performs an opaque
// call and an ambient read inside an artifact_group. The Phase-1 admission pass
// fails closed with HYB-E4702 (opaque call) and HYB-E4701 (ambient read/write).
package main

import "fmt"

artifact_group("bad-worker", budget = 64):
    artifact_worker("leaky"):
        t := time.now()
        x := opaque_call(t)
        return x, []
    publish_manifest()

fn main():
    fmt.Println("rejected at admission")