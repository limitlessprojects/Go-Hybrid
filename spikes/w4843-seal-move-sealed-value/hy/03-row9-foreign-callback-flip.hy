// W-4843 Reproducer 03 — GUARANTEES.md Rows 5 & 9 flip (HYB-E3013).
//
// Today a closure that only *reads* a mutable local inside a foreign Go API
// callback passes `hyb check` (HYB-E3013 only checks captured writes), yet a
// later write to the same binding in the parent function is a real
// concurrent map read/write crash. Requiring the capture to be `const` or
// `sealed[T]` closes the hole.

fn register_unsealed_reject() {
    routes := map[string]int{"/x": 1}
    http.handle_func("/x", fn(w, r) {
        _ = routes["/x"]       // REJECT HYB-E3013: `routes` is neither const nor sealed.
    })
    routes["/admin"] = 403     // the mutation HYB-E3013 exists to catch.
}

fn register_sealed_accept() {
    routes := map[string]int{"/x": 1}
    snap := seal move routes
    http.handle_func("/x", fn(w, r) {
        _ = snap["/x"]          // ACCEPT: `snap` is sealed[map[string]int]; no writer exists anywhere.
    })
}

fn main() {
    register_sealed_accept()
}
