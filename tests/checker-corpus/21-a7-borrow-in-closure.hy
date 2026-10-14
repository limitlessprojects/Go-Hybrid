// EXPECTED-VERDICT: REJECT LOAN-ESCAPE
// Rationale: A7/LOAN-ESCAPE. A closure capturing a borrow-handle from the outer scope
// and escaping (e.g., passed to a task or returned) violates the rule that loans
// never escape their defining frame. This test ensures the checker detects
// borrow-handles leaking into closures that outlive the borrow's validity.

task worker(f func()) {
    f()
}

fn main() {
    x := 10
    {
        y := &x
        // Closure captures y (a borrow of x)
        // Passing this closure to a task allows the borrow to escape the block
        spawn worker(fn() {
            print(*y)
        })
    }
}