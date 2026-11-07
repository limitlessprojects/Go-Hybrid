// Example 8: Error handling with early return
// Demonstrates: proper cleanup on error paths

package main

func process_request(id: string) -> Result {
    scope {
        h_validate := task validate(id)
        if !join(move h_validate) {
            return Result{error: "invalid id"}
        }  // h_validate already consumed, scope can exit
        
        h_fetch := task fetch_data(id)
        data, err := join(move h_fetch)
        if err != nil {
            return Result{error: err}
        }
        
        return Result{data: data}
    }
}

func validate(id: string) -> bool { return id != "" }
func fetch_data(id: string) -> (string, error) { return "data", nil }
