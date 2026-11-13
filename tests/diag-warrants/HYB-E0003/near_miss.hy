fn main() { managed("shared map across tasks", "mutex") { counts := map[string]int{} } }
