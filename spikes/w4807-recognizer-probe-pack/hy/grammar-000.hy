// Revision code 000. Probes "+0", "00", and "1_0" reveal the three
// independent flags; this revision accepts none of those answer vectors.
pub fn recognize(input []byte) bool {
    var seen int = 0
    for b in input {
        if b == 43 { seen |= 4 }
        if b == 48 { seen |= 2 }
        if b == 95 { seen |= 1 }
    }
    return false
}
