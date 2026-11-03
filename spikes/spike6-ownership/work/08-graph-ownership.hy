// ============================================================================
// 08-graph-ownership — Spike 6: examples/08-graph-ownership under Tier-0
// rules. THE ownership-model stress case: a CYCLIC graph. Semantics and
// expected output unchanged (expected.txt):
//     a reaches d: true
//     d reaches a: false
//
// ---- Tier-0 rule set applied to each value ----
// Layer 1 (Go-side GC): `graph`, `nodes` (map — the ARENA that owns all
//   nodes), `node.edges` ([]node_id), `seen` (set), `stack` ([]node_id).
//   The cycle a->b->c->a lives INSIDE the map, referenced by id — there is
//   no shared mutable borrow anywhere in the program. This is exactly the
//   shape Tier 0 is designed to make "fall out" (research/ownership-models.md).
// Layer 2 (Rust-side owned): none.
// Layer 3 (boundary handles): none.
//
// Loans traced (all function-local, non-lexical):
//   - connect: L1 = `n := &g.nodes[from]` — a UNIQUE map-slot loan, live
//     from creation to its LAST USE (`n.edges = append(...)` on the next
//     line). WINDOW CHECK under the strictest reading (RESULTS A3): no map
//     insert/delete occurs inside the window — `append` reallocates the
//     node's edge SLICE, never the map — so no rehash can invalidate L1.
//   - reachable: value receiver `g graph` = shallow copy (map header is
//     shared, GC semantics); `g.nodes[cur].edges` reads are copies (A4).
//
// Codegen note (not an ownership issue, RESULTS A3): Go has no `&m[k]`
// (map slots are not addressable); the compiler lowers slot-borrows to a
// read-modify-write (copy out, mutate, store back). Semantics identical.
//
// ---- Sendability check sites (task spawn / channel send) ----
//   NONE.
//
// ---- Annotations / managed ----
//   lifetime annotations: 0 — the hard case needs none, because ids replace
//   shared borrows. `managed` blocks: 0 — the escape hatch is NOT used;
//   id-based edges are the safe pattern (as the sketch header says).
// ============================================================================

struct node {
    value int
    edges []node_id            // references by id: no shared mutable borrows
}

struct graph {
    nodes map[node_id]node     // graph owns all nodes (arena discipline)
}

type node_id = int

fn (g *graph) add_node(value int) node_id {
    id := node_id(len(g.nodes))
    g.nodes[id] = node{value: value}
    return id
}

fn (g *graph) connect(from node_id, to node_id) {
    n := &g.nodes[from]        // LOAN L1: unique map-slot borrow, block-scoped
                               // live: creation -> last use (non-lexical)
    n.edges = append(n.edges, to) // LAST USE of n: L1 dies here. No map
                                  // insert/delete inside the window (A3).
}

fn (g graph) reachable(from node_id, target node_id) bool {
    if from == target { return true }
    seen := set[node_id]{}
    stack := []node_id{from}
    for len(stack) > 0 {
        cur := stack.pop()
        if !seen.insert(cur) { continue } // already visited
        if cur == target { return true }
        for e in g.nodes[cur].edges {     // read: copy (A4)
            stack = append(stack, e)
        }
    }
    return false
}

fn main() {
    g := graph{nodes: map[node_id]node{}}
    a := g.add_node(1)         // ids: 0,1,2,3
    b := g.add_node(2)
    c := g.add_node(3)
    d := g.add_node(4)

    g.connect(a, b)
    g.connect(b, c)
    g.connect(c, a)            // CYCLE: a->b->c->a — the hard case
    g.connect(c, d)

    fmt.println("a reaches d: {g.reachable(a, d)}")
    fmt.println("d reaches a: {g.reachable(d, a)}")
}
