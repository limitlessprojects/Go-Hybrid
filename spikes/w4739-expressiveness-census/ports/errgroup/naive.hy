import "context"
import "fmt"
import "sync"

struct token {}

struct group {
    cancel fn(error)
    wg sync.waitgroup
    sem chan token
    err_once sync.once
    err error
}

fn (g *group) done() {
    if g.sem != nil { <-g.sem }
    g.wg.done()
}

fn with_context(ctx context.context) (*group, context.context) {
    ctx, cancel := context.with_cancel_cause(ctx)
    return &group{cancel: cancel}, ctx
}

fn (g *group) wait() error {
    g.wg.wait()
    if g.cancel != nil { g.cancel(g.err) }
    return g.err
}

fn (g *group) spawn(f fn() error) {
    if g.sem != nil { g.sem <- token{} }
    g.wg.add(1)
    task fn() {
        defer g.done()
        err := f()
        if err != nil {
            g.err_once.do(fn() {
                g.err = err
                if g.cancel != nil { g.cancel(g.err) }
            })
        }
    }()
}

fn (g *group) try_spawn(f fn() error) bool {
    if g.sem != nil {
        select {
            case g.sem <- token{}: {}
            default: { return false }
        }
    }
    g.wg.add(1)
    task fn() {
        defer g.done()
        err := f()
        if err != nil {
            g.err_once.do(fn() {
                g.err = err
                if g.cancel != nil { g.cancel(g.err) }
            })
        }
    }()
    return true
}

fn (g *group) set_limit(n int) {
    if n < 0 { g.sem = nil; return }
    if len(g.sem) != 0 {
        panic(fmt.errorf("errgroup: modify limit while %v goroutines in the group are still active", len(g.sem)))
    }
    g.sem = make(chan token, n)
}
