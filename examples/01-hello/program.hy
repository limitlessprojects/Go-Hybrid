// 01-hello — stresses: basic formatting, string interpolation, zero-value init.
// TARGET SYNTAX (pre-MVP; see ../README.md status note). Expected output: expected.txt

fn main() {
    name := "Hybrid"
    count := 0                    // zero value: no nil, no null
    count += 1

    fmt.println("hello, {name}!")       // interpolation
    fmt.printf("count=%d ok=%v\n", count, count == 1)

    let langs = ["Go", "Rust", "Hybrid"]
    for i, l in langs {
        fmt.println("{i}: {l}")
    }
}
