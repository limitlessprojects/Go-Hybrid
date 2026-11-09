// Example 5: Worker pool with structured lifecycle
// Demonstrates: dynamic task creation in loops

package main

func main() {
    jobs := make(chan Job, 100)
    results := make(chan Result, 100)
    
    scope {
        // Spawn worker pool
        for i in 0..4 {
            h := task worker(i, jobs, results)
            join(move h)  // This won't work - need different pattern
        }
        
        // Send jobs
        for j in 0..20 {
            jobs <- Job{id: j}
        }
        close(jobs)
        
        // Collect results
        for r in results {
            println("Result: {r}")
        }
    }
}

func worker(id: int, jobs: chan Job, results: chan Result) {
    for j in jobs {
        result := process(j)
        results <- result
    }
}

func process(j: Job) -> Result { return Result{id: j.id, value: j.id * 2} }
