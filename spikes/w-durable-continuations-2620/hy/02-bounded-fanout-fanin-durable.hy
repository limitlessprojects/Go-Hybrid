// W-2620 (#2620): Admitted durable region — Bounded 3-Worker Fan-Out / Fan-In
// Deterministic round-robin scheduler + bounded FIFO queues preserve in-flight items across cuts.

struct work_unit {
     shard int
     payload int
}

// @[durable_region(id = "fanout_v1", workers = 3, checkpoint_every = 4)]
fn run_durable_fanout(total int) int {
    in_q := make(chan work_unit, 6)
    out_q := make(chan int, 6)
    res_q := make(chan int, 1)

    task dispatcher(total, in_q)
    task worker_step(0, in_q, out_q)
    task worker_step(1, in_q, out_q)
    task worker_step(2, in_q, out_q)
    task collector(total, out_q, res_q)

    return <-res_q
}
