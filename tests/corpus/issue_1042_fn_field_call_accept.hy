// EXPECTED-VERDICT: ACCEPT
// Issue #1042 control: indirect function-field calls Returning primitive values
// remain sendable and buildable across tasks.

struct calc {
    op fn(x int) int
}

fn worker(v int, done chan<- int) {
    done <- v + 1
}

fn main() {
    c := calc{
        op: fn(x int) int {
            return x * 2
        },
    }
    res := c.op(20)
    done := make(chan int, 1)
    task worker(res, done)
    out := <-done
    fmt.println("{out}")
}
