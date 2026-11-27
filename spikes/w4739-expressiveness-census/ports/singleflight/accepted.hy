// BSD-derived production port: see upstream/LICENSE and PREREGISTRATION.md.
import "sync"

struct call {
    wg sync.waitgroup
    val any
    err error
    dups int
    deliveries []fn(any, error, bool)
}

struct group {
    mu sync.mutex
    m sync.map_
}

fn (g *group) do(key string, work fn() any error, exit_error error,
    is_panic fn(error) bool, make_panic fn(any) error, exit_fn fn(),
    lookup fn(store *sync.map_, key string) *call, finish fn(c *call, duplicate bool),
    put fn(store *sync.map_, key string, c *call), drop fn(store *sync.map_, key string)) {
    managed("map protected by mutex, completed call published by WaitGroup", "mutex", "GH-4739") {
        g.mu.lock()
        var c *call = lookup(&g.m, key)
        if c != nil {
            c.dups++
            g.mu.unlock()
            c.wg.wait()
            if is_panic(c.err) { panic(c.err) }
            if c.err == exit_error { exit_fn() }
            finish(c, true)
            return
        }
        c = new(call)
        c.wg.add(1)
        put(&g.m, key, c)
        g.mu.unlock()
        g.do_call(c, key, work, exit_error, is_panic, make_panic, exit_fn, lookup, finish, put, drop)
        finish(c, false)
    }
}

fn (g *group) do_chan(key string, work fn() any error, deliver fn(any, error, bool),
    exit_error error, is_panic fn(error) bool, make_panic fn(any) error, exit_fn fn(),
    lookup fn(store *sync.map_, key string) *call, finish fn(c *call, duplicate bool),
    put fn(store *sync.map_, key string, c *call), drop fn(store *sync.map_, key string)) {
    managed("callbacks send to original buffered Go result channels; mutex guards map", "interop", "GH-4739") {
        g.mu.lock()
        var c *call = lookup(&g.m, key)
        if c != nil {
            c.dups++
            c.deliveries = append(c.deliveries, deliver)
            g.mu.unlock()
            return
        }
        c = new(call)
        c.deliveries = append(c.deliveries, deliver)
        c.wg.add(1)
        put(&g.m, key, c)
        g.mu.unlock()
        task g.do_call(c, key, work, exit_error, is_panic, make_panic, exit_fn, lookup, finish, put, drop)
    }
}

fn (g *group) do_call(c *call, key string, work fn() any error, exit_error error,
    is_panic fn(error) bool, make_panic fn(any) error, exit_fn fn(),
    lookup fn(store *sync.map_, key string) *call, finish fn(c *call, duplicate bool),
    put fn(store *sync.map_, key string, c *call), drop fn(store *sync.map_, key string)) {
    managed("retain double-defer panic versus Goexit protocol; Go callbacks are escapes", "interop", "GH-4739") {
        normal_return := false
        recovered := false
        defer fn() {
            if !normal_return && !recovered { c.err = exit_error }
            g.mu.lock()
            defer g.mu.unlock()
            c.wg.done()
            if lookup(&g.m, key) == c { drop(&g.m, key) }
            if is_panic(c.err) {
                if len(c.deliveries) > 0 {
                    managed("panic must terminate the process as in upstream DoChan", "interop", "GH-4739") {
                        task fn() { panic(c.err) }()
                    }
                    select {}
                } else {
                    panic(c.err)
                }
            } else {
                if c.err != exit_error {
                    for _, deliver in c.deliveries {
                        deliver(c.val, c.err, c.dups > 0)
                    }
                }
            }
        }()
        fn() {
            defer fn() {
                if !normal_return {
                    r := recover()
                    if r != nil { c.err = make_panic(r) }
                }
            }()
            c.val, c.err = work()
            normal_return = true
        }()
        if !normal_return { recovered = true }
    }
}

fn (g *group) forget(key string, drop fn(store *sync.map_, key string)) {
    g.mu.lock()
    drop(&g.m, key)
    g.mu.unlock()
}
