// W-2220 M2 negative falsifier for row 14.
//
// SPEC-LANGUAGE.md §9.9 (O20): `unsafe` is reserved, not implemented, in the
// MVP. There is no memory-unsafe escape in safe mode — the keyword itself is
// rejected, so the claim "no raw cross-language pointer machinery is granted"
// is observable instead of asserted.
//
// EXPECTED-VERDICT: REJECT

fn main() {
    unsafe {
        x := 1
        fmt.println(x)
    }
}
