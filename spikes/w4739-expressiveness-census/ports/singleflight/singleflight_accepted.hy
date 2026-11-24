package singleflight_pilot

import "fmt"

struct request_msg {
    key      string
    reply_ch chan result_msg
}

struct result_msg {
    data string
    err  string
}

fn coordinator_loop(req_ch chan request_msg) {
    in_flight := make(map[string][]chan result_msg)

    for {
        req := <-req_ch
        waiters, exists := in_flight[req.key]
        if exists {
            in_flight[req.key] = append(waiters, req.reply_ch)
            continue
        }

        in_flight[req.key] = []chan result_msg{req.reply_ch}
        task execute_single(req.key, req_ch)
    }
}

fn execute_single(key string, req_ch chan request_msg) {
    // Computes unique key and returns to waiters
    res := result_msg{data: fmt.sprintf("val:%s", key), err: ""}
    // Dispatches result back
}

pub struct FlightGroup {
    req_ch chan request_msg
}

pub fn new_flight_group() *FlightGroup {
    ch := make(chan request_msg, 64)
    task coordinator_loop(ch)
    return &FlightGroup{req_ch: ch}
}
