// Structurally different, language-equivalent baseline. Its extra state
// toggles on every byte but acceptance is still always false.
pub fn recognize(input []byte) bool {
    var state int = 0
    for b in input {
        if state == 0 {
            state = 1
        } else {
            state = 0
        }
    }
    return false
}
