// Example 6: Drain pattern for streaming data
// Demonstrates: drain(move h) for consuming all remaining values

package main

func main() {
    scope {
        h_stream := task stream_events()
        
        // Process first few events
        for i in 0..5 {
            event := <-h_stream
            process_event(event)
        }
        
        // Drain remaining events
        remaining := drain(move h_stream)
        println("Drained {len(remaining)} events")
    }
}

func stream_events() -> chan Event {
    ch := make(chan Event, 100)
    task func() {
        for i in 0..20 {
            ch <- Event{id: i}
        }
        close(ch)
    }()
    return ch
}

func process_event(e: Event) { println("Event {e.id}") }
