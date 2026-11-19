// 08-graph-ownership — stresses: the OWNERSHIP MODEL on cyclic structures —
// the case plain borrowing cannot express. Node storage is owned by the graph
// (arena-style); edges reference node ids (indices), never raw pointers.
// `managed` escape hatch is NOT used: id-based edges are the safe pattern.
// TARGET SYNTAX.

struct node {
    value  int
    edges  []node_id              // references by id: no shared mutable borrows
}

struct graph {
    nodes map[node_id]node        // graph owns all nodes (arena discipline)
}

type node_id = int

fn (g *graph) add_node(value int) node_id {
    id := node_id(len(g.nodes))
    g.nodes[id] = node{value: value}
    return id
}

fn (g *graph) connect(from node_id, to node_id) {
    n := &g.nodes[from]           // unique mutable borrow, scoped to the block
    n.edges = append(n.edges, to)
}

fn (g graph) reachable(from node_id, target node_id) bool {
    if from == target { return true }
    seen := set[node_id]{}
    stack := []node_id{from}
    for len(stack) > 0 {
        cur := stack.pop()
        if !seen.insert(cur) { continue }   // already visited
        if cur == target { return true }
        for e in g.nodes[cur].edges {
            stack = append(stack, e)
        }
    }
    return false
}

fn main() {
    g := graph{nodes: map[node_id]node{}}
    a := g.add_node(1)            // ids: 0,1,2,3
    b := g.add_node(2)
    c := g.add_node(3)
    d := g.add_node(4)

    g.connect(a, b)
    g.connect(b, c)
    g.connect(c, a)               // CYCLE: a->b->c->a — the hard case
    g.connect(c, d)

    fmt.println("a reaches d: {g.reachable(a, d)}")
    fmt.println("d reaches a: {g.reachable(d, a)}")
}
