// Example 10: Pipeline with staged processing
// Demonstrates: chained scopes for data transformation pipeline

package main

func main() {
    input := make(chan int, 100)
    
    scope {
        // Stage 1: Generate
        h_gen := task generate(input)
        
        scope {
            // Stage 2: Transform
            transformed := make(chan int, 100)
            h_trans := task transform(input, transformed)
            
            scope {
                // Stage 3: Aggregate
                h_agg := task aggregate(transformed)
                result := join(move h_agg)
                println("Final result: {result}")
            }
            
            join(move h_trans)
        }
        
        join(move h_gen)
    }
}

func generate(out: chan int) {
    for i in 0..100 {
        out <- i
    }
    close(out)
}

func transform(in: chan int, out: chan int) {
    for v in in {
        out <- v * 2
    }
    close(out)
}

func aggregate(in: chan int) -> int {
    sum := 0
    for v in in {
        sum += v
    }
    return sum
}
