// Example 4: Cancellation pattern for timeout handling
// Demonstrates: explicit cancellation with cancel(move h)

package main

func main() {
    scope {
        h_slow := task slow_operation()
        h_timeout := task timeout_after(5 * time.seconds)
        
        select {
            result := <-h_slow:
                cancel(move h_timeout)  // Cancel timeout since we got result
                println("Completed: {result}")
            <-h_timeout:
                cancel(move h_slow)  // Cancel slow op since we timed out
                println("Timed out")
        }
    }
}

func slow_operation() -> string { 
    time.sleep(10 * time.seconds)
    return "done" 
}
func timeout_after(duration: time.Duration) -> chan time.Time {
    return time.after(duration)
}
