const f = 2.5

fn main() {
    for i in 0..nil { _ = i }
    for i in 0..(1 < 2) { _ = i }
    for i in 0..f { _ = i }

    var n u32 = 5
    for i in 0..n { _ = i }

    var lo i32 = 0
    var hi i64 = 5
    for i in lo..hi { _ = i }

    for i in 0..(1 << 63) { _ = i }
}
