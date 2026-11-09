// Example 9: Race pattern with first-wins semantics
// Demonstrates: multiple tasks, consume first result, cancel others

package main

func main() {
    scope {
        h1 := task query_server_a()
        h2 := task query_server_b()
        h3 := task query_server_c()
        
        select {
            result := <-h1:
                cancel(move h2)
                cancel(move h3)
                println("Server A responded first: {result}")
            result := <-h2:
                cancel(move h1)
                cancel(move h3)
                println("Server B responded first: {result}")
            result := <-h3:
                cancel(move h1)
                cancel(move h2)
                println("Server C responded first: {result}")
        }
    }
}

func query_server_a() -> string { time.sleep(100 * time.ms); return "A" }
func query_server_b() -> string { time.sleep(200 * time.ms); return "B" }
func query_server_c() -> string { time.sleep(150 * time.ms); return "C" }
