fn main() { ch := make(chan *int); ptr := new(int); ch <- ptr }
