import "bytes"
import "sort"

fn inspect_only() -> bool {
    const a: []u8 = []u8("abc")
    const b: []u8 = []u8("abc")
    return bytes.equal(a, b)
}

fn mutate_const_rejected() {
    const xs: []int = [3, 1, 2]
    sort.ints(xs)
}
