// W-605 (Issue #605) sample 02: REJECT (`HYB-E3601` / `HYB-E3602`) — Unmediated
// wall-clock read (`time.now()`) and `managed(...)` escape block inside a
// `replay_island` region fail closed during Phase-1 admission.
package main

import "time"

fn unmediated_island():
    managed("escape replay coordinator", "w605-spike"):
        now := time.now()
        _ = now
