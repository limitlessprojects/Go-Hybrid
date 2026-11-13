fn main() { var s []int; { r1 := &mut s; _ = r1 }; r2 := &mut s; _ = r2 }
