// Accept exactly the one-byte input B.
pub fn recognize(input []byte) bool {
    var state int = 0
    for b in input {
        if state == 0 {
            if b == 66 {
                state = 1
            } else {
                state = 2
            }
        } else {
            state = 2
        }
    }
    return state == 1
}
