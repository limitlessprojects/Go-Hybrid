// GoBench serving/5865: `go rw.run()` — a method spawn on a pointer receiver.
struct revision_watcher {
    dests_ch chan int
}

fn (rw *revision_watcher) run() {
    close(rw.dests_ch)
}

fn main() {
    ch := make(chan int, 1)
    rw := revision_watcher{dests_ch: ch}
    task rw.run()
}