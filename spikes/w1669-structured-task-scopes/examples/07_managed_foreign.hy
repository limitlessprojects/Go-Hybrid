// Example 7: Managed foreign callback (HTTP handler)
// Demonstrates: HYB-E3503 escape hatch with managed()

package main

import "net/http"

func main() {
    managed("HTTP request handler", "foreign_callback")
    http.HandleFunc("/api/data", fn(req: http.Request) -> http.Response {
        scope {
            h := task fetch_from_db(req.id)
            data := join(move h)
            return http.Response{body: data}
        }
    })
    
    http.ListenAndServe(":8080", nil)
}

func fetch_from_db(id: string) -> string {
    return "data for {id}"
}
