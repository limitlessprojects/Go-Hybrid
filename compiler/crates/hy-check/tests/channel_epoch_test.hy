fn main() {
tx, rx := chan[int](10);
task worker(move tx);
}

fn worker(tx tx[int]) {
tx <- 42;
close(move tx);
}

fn receiver(rx rx[int]) {
println(<-rx);
}