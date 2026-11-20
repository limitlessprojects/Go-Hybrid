// W-3884 (Issue #3884) sample 02: REJECT (HYB-E3884-SCOPE-ESCAPE)
// Attempting to return an ephemeral RegionCap[R] witness outside its defining
// `enter_region` scope is rejected statically by `hy-check` to preserve soundness.
package main

import "hybrid/membrane"

fn leak_capability() membrane.RegionCap[membrane.RegionA]:
    var leaked membrane.RegionCap[membrane.RegionA]
    membrane.enter_region[membrane.RegionA](fn(capA membrane.RegionCap[membrane.RegionA]):
        leaked = capA // ERROR HYB-E3884-SCOPE-ESCAPE: capability witness cannot escape enclosing region block
    )
    return leaked

fn main():
    _ = leak_capability()
