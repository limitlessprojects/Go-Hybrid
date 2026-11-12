// 03-noescape-loop-reuse-admitted.hy
// EXPECTED-VERDICT: ACCEPT (escape-certified stack scratch slot reset at loop back-edge; F2 interface sink rejected)

@[no_escape_loop]
fn sum_packets_stack(iterations int) int64 {
    var pkt Packet
    var acc int64 = 0
    for i := 0..iterations {
        pkt.reset()
        acc = acc + int64(i)
    }
    return acc
}
