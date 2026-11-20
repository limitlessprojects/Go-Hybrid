// GoBench moby/27037, lifted: the spawn is a closure capturing the loop variable.
// Minimal lift = hoist the body into a top-level fn and pass the capture with move.
fn report(i int) {
    _ = i
}

fn main() {
    for i in [17, 18, 19, 20, 21] {
        task report(move i)
    }
}