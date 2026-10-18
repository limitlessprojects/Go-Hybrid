// EXPECTED-CANCEL: E8002 x1
// Two receive arms, neither a token nor a timer: the select can block
// forever after main closes `done`.
fn worker(done chan bool, a chan int, b chan int) {
    select {
        v := <-a: { fmt.println("{v}") }
        w := <-b: { fmt.println("{w}") }
    }
    <-done
}

fn main() {
    done := make(chan bool)
    a := make(chan int)
    b := make(chan int)
    task worker(done, a, b)
    close(done)
}
