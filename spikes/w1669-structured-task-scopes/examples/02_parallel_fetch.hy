// Example 2: Multiple concurrent tasks with ordered joins
// Demonstrates: parallel execution with deterministic completion

package main

func main() {
    scope {
        h1 := task fetch_user()
        h2 := task fetch_orders()
        h3 := task fetch_preferences()
        
        user := join(move h1)
        orders := join(move h2)
        prefs := join(move h3)
        
        render_page(user, orders, prefs)
    }
}

func fetch_user() -> User { return User{name: "Alice"} }
func fetch_orders() -> []Order { return []Order{} }
func fetch_preferences() -> Preferences { return Preferences{} }
func render_page(u: User, o: []Order, p: Preferences) { }
