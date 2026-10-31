// Example 3: Nested scopes for hierarchical task structure
// Demonstrates: parent-child task relationships

package main

func main() {
    scope {
        h_outer := task setup_connection()
        conn := join(move h_outer)
        
        scope {
            h_query1 := task execute_query(conn, "SELECT * FROM users")
            h_query2 := task execute_query(conn, "SELECT * FROM orders")
            
            users := join(move h_query1)
            orders := join(move h_query2)
            
            process_results(users, orders)
        }
        
        close_connection(conn)
    }
}

func setup_connection() -> Connection { return Connection{} }
func execute_query(conn: Connection, sql: string) -> []Row { return []Row{} }
func process_results(users: []Row, orders: []Row) { }
func close_connection(conn: Connection) { }
