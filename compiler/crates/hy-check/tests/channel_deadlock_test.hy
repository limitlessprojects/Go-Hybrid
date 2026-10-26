fn main() {
tx1, rx1 := chan[int](10);
tx2, rx2 := chan[int](10);
task worker1(move tx1, rx2);
task worker2(move tx2, rx1);
}

fn worker1(tx tx[int], rx rx[int]) {
tx <- 42;
println(<-rx);
close(move tx);
}

fn worker2(tx tx[int], rx rx[int]) {
tx <- 42;
println(<-rx);
close(move tx);
}