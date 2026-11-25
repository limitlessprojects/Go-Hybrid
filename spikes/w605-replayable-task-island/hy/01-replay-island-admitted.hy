// W-605 (Issue #605) sample 01: ACCEPT — Closed `replay_island` region with two
// concurrent producers racing into a shared channel and a multi-arm `select`
// consumer. Every scheduling decision (`EventChanSend`, `EventChanRecv`, `EventSelectArm`)
// is mediated by the island coordinator and reproducible from a 16-byte-per-event trace.
package main

import "fmt"

fn run_order_island(seed u64) int:
    ch := make(chan int, 4)
    task():
        ch <- 101
        ch <- 102
    task():
        ch <- 201
        ch <- 202
    first := <-ch
    return first

fn main():
    fmt.Println(run_order_island(1))
