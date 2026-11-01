// Contrast with tests/checker-corpus/01-shared-map-across-tasks.hy (REJECTED by Tier-0 with HYB-E3001).
// Inside an opted-in `tx_region`, shared mutable composite accesses are mediated by
// versioned transactional cells (`TxMap[string, int]`) and committed with serializable MVCC.
package main

fn run_shared_map_tx() int {
    tx_region {
        counts := tx_map[string, int]{"hits": 10, "a": 1, "b": 2}

        t1 := task (counts) {
            h := counts.get("hits")
            counts.put("hits", h + 5)
            counts.put("a", counts.get("hits") * 2)
        }

        t2 := task (counts) {
            h := counts.get("hits")
            counts.put("hits", h * 3)
            counts.put("b", counts.get("hits") + 1)
        }

        await t1
        await t2
        return counts.get("hits")
    }
}
