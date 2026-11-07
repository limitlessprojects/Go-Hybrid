# Migration Guide: Go to Hybrid Structured Concurrency

This guide helps you migrate existing Go code with goroutines to Hybrid's structured task scopes.

## Why Migrate?

Go's unstructured `go` statements can lead to:
- Goroutine leaks (the #1 source of memory bloat in production Go)
- Lost context cancellations
- Difficult-to-reason-about concurrency

Hybrid's `scope { ... }` blocks provide **compile-time guarantees** against these issues.

## Migration Patterns

### Pattern 1: Simple Fire-and-Forget → Structured Scope

**Before (Go):**
```go
func processRequest(id string) {
    go func() {
        // Background work
        result := doWork(id)
        save(result)
    }()
    // Function returns immediately, goroutine may leak
}
```

**After (Hybrid):**
```hy
func processRequest(id: string) {
    scope {
        h := task do_work_async(id)
        result := join(move h)
        save(result)
    }  // Compile-time guarantee: h is joined before exit
}

func do_work_async(id: string) -> Result {
    return doWork(id)
}
```

### Pattern 2: WaitGroup → Scope with Multiple Tasks

**Before (Go):**
```go
func fetchAll() {
    var wg sync.WaitGroup
    wg.Add(3)
    
    go func() {
        defer wg.Done()
        fetchUser()
    }()
    
    go func() {
        defer wg.Done()
        fetchOrders()
    }()
    
    go func() {
        defer wg.Done()
        fetchPreferences()
    }()
    
    wg.Wait()
}
```

**After (Hybrid):**
```hy
func fetchAll() {
    scope {
        h1 := task fetchUser()
        h2 := task fetchOrders()
        h3 := task fetchPreferences()
        
        join(move h1)
        join(move h2)
        join(move h3)
    }  // All handles must be consumed
}
```

### Pattern 3: Context Cancellation → Explicit Cancel

**Before (Go):**
```go
func withTimeout(ctx context.Context) {
    ctx, cancel := context.WithTimeout(ctx, 5*time.Second)
    defer cancel()
    
    go func() {
        select {
        case <-ctx.Done():
            return
        case result := <-doWork():
            save(result)
        }
    }()
}
```

**After (Hybrid):**
```hy
func withTimeout() {
    scope {
        h_work := task doWork()
        h_timeout := task timeout_after(5 * time.seconds)
        
        select {
            result := <-h_work:
                cancel(move h_timeout)
                save(result)
            <-h_timeout:
                cancel(move h_work)
                println("Timed out")
        }
    }
}
```

### Pattern 4: Worker Pool → Scoped Worker Lifecycle

**Before (Go):**
```go
func workerPool(jobs <-chan Job, results chan<- Result) {
    var wg sync.WaitGroup
    
    for i := 0; i < 4; i++ {
        wg.Add(1)
        go func(id int) {
            defer wg.Done()
            for j := range jobs {
                results <- process(j)
            }
        }(i)
    }
    
    wg.Wait()
    close(results)
}
```

**After (Hybrid):**
```hy
func workerPool(jobs: chan Job, results: chan Result) {
    scope {
        // Spawn workers
        handles := []
        for i in 0..4 {
            h := task worker(i, jobs, results)
            handles.append(h)
        }
        
        // Wait for all workers
        for h in handles {
            join(move h)
        }
    }
    
    close(results)
}

func worker(id: int, jobs: chan Job, results: chan Result) {
    for j in jobs {
        results <- process(j)
    }
}
```

### Pattern 5: HTTP Handler → Managed Foreign Callback

**Before (Go):**
```go
http.HandleFunc("/api", func(w http.ResponseWriter, r *http.Request) {
    go func() {
        // Background processing
        result := processRequest(r)
        log(result)
    }()
    w.Write([]byte("OK"))
})
```

**After (Hybrid):**
```hy
managed("HTTP request handler", "foreign_callback")
http.HandleFunc("/api", fn(req: http.Request) -> http.Response {
    scope {
        h := task processRequest(req)
        result := join(move h)
        log(result)
        return http.Response{body: "OK"}
    }
})
```

## Common Pitfalls

### ❌ Don't: Spawn in Loop Without Joining

```hy
// WRONG - will fail with HYB-E3501
scope {
    for i in 0..10 {
        h := task worker(i)
        // h not joined!
    }
}
```

### ✅ Do: Join Each Handle

```hy
// CORRECT
scope {
    for i in 0..10 {
        h := task worker(i)
        join(move h)
    }
}
```

### ❌ Don't: Conditional Spawn Without Guaranteed Join

```hy
// WRONG - may fail with HYB-E3501
scope {
    if condition {
        h := task worker()
    }
    // h not in scope here
}
```

### ✅ Do: Spawn Outside Conditional

```hy
// CORRECT
scope {
    h := task worker()
    if condition {
        join(move h)
    } else {
        cancel(move h)
    }
}
```

## Automated Migration Tool (Future Work)

We plan to provide a `go2hy` tool that:
1. Detects `go func()` patterns
2. Suggests `scope { ... }` replacements
3. Validates that all handles are joined
4. Generates Hybrid code from Go source

## Checklist

Before submitting your migrated code:

- [ ] All `go` statements replaced with `task` inside `scope`
- [ ] All `sync.WaitGroup` patterns converted to scope joins
- [ ] All context cancellations converted to explicit `cancel(move h)`
- [ ] Foreign callbacks wrapped with `managed(..., "foreign_callback")`
- [ ] Code compiles with zero HYB-E3501/E3502/E3503 diagnostics
- [ ] All tests pass
- [ ] No goroutine leaks in `-race` testing

## Resources

- [SPEC-LANGUAGE.md §8.6](../../SPEC-LANGUAGE.md) — Formal specification
- [Examples](./examples/) — 10 working examples
- [Adversarial Corpus](./tests/) — 25 leaky patterns to avoid
