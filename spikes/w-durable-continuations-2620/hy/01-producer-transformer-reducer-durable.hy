// W-2620 (#2620): Admitted durable region — Producer -> Transformer -> Reducer
// Closed finite task graph, bounded FIFO channels, scalar/value records only.

struct item_rec {
    seq int
    val int
}

// @[durable_region(id = "ptr_v1", checkpoint_every = 3)]
fn run_durable_pipeline(n int) int {
    raw_q := make(chan item_rec, 4)
     scaled_q := make(chan item_rec, 4)
    done_q := make(chan int, 1)

    task producer(n, raw_q)
    task transformer(n, raw_q, scaled_q)
    task reducer(n, scaled_q, done_q)

    return <-done_q
}
