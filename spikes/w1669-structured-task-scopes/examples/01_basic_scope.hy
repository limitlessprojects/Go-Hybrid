// Example 1: Basic structured scope with single task
// Demonstrates: simple spawn-join pattern

package main

func main() {
    scope {
        h := task fetch_data()
        result := join(move h)
        println("Fetched: {result}")
    }
}

func fetch_data() -> string {
    return "data from async source"
}
