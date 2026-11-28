// W-4843 Reproducer 02 — REJECT: HYB-E3004 (non-owned binding) and
// HYB-E3015 (non-sealable interior closure, declared falsifier F3).

struct config {
    next_id fn() int
}

fn borrow_reject(routes map[string]int) {
    snap := seal move routes   // REJECT HYB-E3004: `routes` is a non-owned parameter.
    _ = snap
}

fn closure_reject() {
    counter := 0
    cfg := config{ next_id: fn() int { counter++; return counter } }

    snap := seal move cfg      // REJECT HYB-E3015: interior closure over mutable state;
    _ = snap                   // calling snap.next_id() would still mutate `counter`.
}

fn main() {
    routes := map[string]int{"/x": 200}
    borrow_reject(routes)
    closure_reject()
}
