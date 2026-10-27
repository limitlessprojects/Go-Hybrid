// ============================================================================
// 136-auto-three-layer-schedule — Auto scheduling corpus #136 (issue #1962).
//
// PURPOSE: complex scheduling scenario with 3 layers.
//
// Task layout:
//   Task 0: writes a        ─┐
//   Task 1: writes b        ─┼─ layer 0 (disjoint, parallel)
//   Task 2: reads a, writes c ─ layer 1 (conflicts with task 0)
//   Task 3: reads c, writes d ─ layer 2 (conflicts with task 2)
//
// Expected layers: [0, 1] → [2] → [3]
// ============================================================================

fn main() {
    a := 0
    b := 0
    c := 0
    d := 0

    auto {
        task {
            a = 10
        }
        task {
            b = 20
        }
        task {
            c = a * 2
        }
        task {
            d = c + 1
        }
    }

    fmt.println("a={a}, b={b}, c={c}, d={d}")
}
