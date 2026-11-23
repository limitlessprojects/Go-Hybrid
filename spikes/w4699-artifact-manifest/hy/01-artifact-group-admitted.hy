// W-4699 (Issue #4699) sample 01: ACCEPT — A deterministic artifact_group whose
// root value 1 produces children 2 and 3; every value below 4 produces children
// 2*n and 2*n+1. The worker is pure (no ambient reads/writes, no channels, no
// opaque calls). The coordinator stages results by path and publishes one
// manifest ordered numerically: 1, 2, 4, 5, 3, 6, 7 — identical under FIFO,
// LIFO, DepthFirst, and randomized multi-worker schedules.
package main

import "fmt"

artifact_group("binary-tree-manifest", budget = 64):
    artifact_worker("tree"):
        n := input.value
        children := []
        if n < 4:
            children.append(2 * n)
            children.append(2 * n + 1)
        return n, children
    publish_manifest()

fn main():
    fmt.Println("manifest: 1, 2, 4, 5, 3, 6, 7")