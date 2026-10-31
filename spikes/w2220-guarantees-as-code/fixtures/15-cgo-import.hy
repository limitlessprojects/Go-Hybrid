// W-2220 M2 negative falsifier for row 15.
//
// cgo is not part of `.hy`: MVP Hybrid imports a curated allowlist of Go
// packages and pinned Rust crates (SPEC M5), so `import "C"` is a compile
// error naming the allowlist rather than a silent link path. Foreign cgo code
// in the surrounding Go project stays outside the checker, exactly as the row
// says.
//
// EXPECTED-VERDICT: REJECT

import "C"

fn main() {
    fmt.println("hi")
}
