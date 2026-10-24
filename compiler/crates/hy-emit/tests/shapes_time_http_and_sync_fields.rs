use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let ast = parse_file("test.hy", src).expect("parse");
    emit_go(&ast).expect("emit_go")
}

#[test]
fn time_methods_and_duration_chains_emit_go_names() {
    let go = emit(
        "fn main() {\n\
         \tt0 := time.now()\n\
         \tt1 := time.now()\n\
         \tms := t1.sub(t0).milliseconds\n\
         \tus := t1.sub(t0).abs().microseconds\n\
         \tut := t0.utc().unix_milli()\n\
         \tz := t0.is_zero()\n\
         \teq := t0.equal(t1)\n\
         \tfmt.println(ms, us, ut, z, eq)\n\
         }\n",
    );
    assert!(
        go.contains("t1.Sub(t0).Milliseconds()"),
        "expected t1.Sub(t0).Milliseconds(), got:\n{}",
        go
    );
    assert!(
        go.contains("t1.Sub(t0).Abs().Microseconds()"),
        "expected t1.Sub(t0).Abs().Microseconds(), got:\n{}",
        go
    );
    assert!(
        go.contains("t0.UTC().UnixMilli()"),
        "expected t0.UTC().UnixMilli(), got:\n{}",
        go
    );
    assert!(
        go.contains("t0.IsZero()") && go.contains("t0.Equal(t1)"),
        "expected t0.IsZero() and t0.Equal(t1), got:\n{}",
        go
    );
}

#[test]
fn http_header_url_values_and_request_selectors_emit_go_names() {
    let go = emit(
        "fn main() {\n\
         \tmux := http.new_serve_mux()\n\
         \tmux.handle(\"/x\", http.handler(fn(w, r) {\n\
         \t\tw.header.del(\"X-Old\")\n\
         \t\tr.url.query.add(\"a\", \"1\")\n\
         \t\tr.url.query.del(\"b\")\n\
         \t\thost := r.url.hostname()\n\
         \t\tsch := r.url.scheme\n\
         \t\tid := r.path_value(\"id\")\n\
         \t\tua := r.user_agent()\n\
         \t\tfmt.println(host, sch, id, ua)\n\
         \t}))\n\
         }\n",
    );
    assert!(
        go.contains("w.Header().Del(\"X-Old\")"),
        "expected w.Header().Del, got:\n{}",
        go
    );
    assert!(
        go.contains("r.URL.Query().Add(\"a\", \"1\")")
            && go.contains("r.URL.Query().Del(\"b\")"),
        "expected url.Values Add/Del, got:\n{}",
        go
    );
    assert!(
        go.contains("r.URL.Hostname()") && go.contains("r.URL.Scheme"),
        "expected r.URL.Hostname() and r.URL.Scheme, got:\n{}",
        go
    );
    assert!(
        go.contains("r.PathValue(\"id\")") && go.contains("r.UserAgent()"),
        "expected r.PathValue and r.UserAgent, got:\n{}",
        go
    );
}

#[test]
fn struct_field_sync_wait_group_once_cond_pool_and_atomic_value_emit_go_methods() {
    let go = emit(
        "struct coordinator {\n\
         \twg sync.wait_group\n\
         \tonce sync.once\n\
         \tcond sync.cond\n\
         \tpool sync.pool\n\
         \tval atomic.value\n\
         }\n\
         \n\
         fn main() {\n\
         \tc := coordinator{}\n\
         \tc.wg.add(1)\n\
         \tc.wg.done()\n\
         \tc.wg.wait()\n\
         \tc.once.do(fn() {})\n\
         \tc.cond.signal()\n\
         \tc.cond.broadcast()\n\
         \tc.pool.put(1)\n\
         \tv := c.pool.get()\n\
         \tc.val.store(42)\n\
         \tfmt.println(v, c.val.load())\n\
         }\n",
    );
    assert!(
        go.contains("c.wg.Add(1)") && go.contains("c.wg.Done()") && go.contains("c.wg.Wait()"),
        "expected sync.WaitGroup Add/Done/Wait on struct field, got:\n{}",
        go
    );
    assert!(
        go.contains("c.once.Do("),
        "expected sync.Once Do on struct field, got:\n{}",
        go
    );
    assert!(
        go.contains("c.cond.Signal()") && go.contains("c.cond.Broadcast()"),
        "expected sync.Cond Signal/Broadcast on struct field, got:\n{}",
        go
    );
    assert!(
        go.contains("c.pool.Put(1)") && go.contains("c.pool.Get()"),
        "expected sync.Pool Put/Get on struct field, got:\n{}",
        go
    );
    assert!(
        go.contains("c.val.Store(42)") && go.contains("c.val.Load()"),
        "expected atomic.Value Store/Load on struct field, got:\n{}",
        go
    );
}
