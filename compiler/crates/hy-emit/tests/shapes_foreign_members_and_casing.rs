use hy_emit::emit_go;
use hy_syntax::parse_file;

fn emit(src: &str) -> String {
    let ast = parse_file("inline.hy", src).unwrap_or_else(|e| panic!("parse failed: {:?}", e));
    emit_go(&ast).unwrap_or_else(|e| panic!("emit failed: {:?}", e))
}

#[test]
fn time_time_and_duration_members_and_chained_results_emit_valid_go() {
    let go_time = emit(
        "import \"time\"\n\
         fn main() {\n\
           t0 := time.now()\n\
           ms := t0.add(time.milliseconds(25)).utc().sub(t0).abs().milliseconds\n\
           ns := t0.unix_nano()\n\
           s := t0.format(\"2006-01-02\")\n\
           z := t0.is_zero()\n\
           fmt.println(ms, ns, s, z)\n\
         }",
    );
    assert!(
        go_time.contains(
            "ms := t0.Add(time.Duration(25) * time.Millisecond).UTC().Sub(t0).Abs().Milliseconds()"
        ),
        "expected chained time.Time/Duration Add/UTC/Sub/Abs/Milliseconds, got:\n{go_time}"
    );
    assert!(
        go_time.contains("ns := t0.UnixNano()")
            && go_time.contains("s := t0.Format(\"2006-01-02\")")
            && go_time.contains("z := t0.IsZero()"),
        "expected t0.UnixNano(), t0.Format(...), and t0.IsZero(), got:\n{go_time}"
    );
}

#[test]
fn http_request_header_and_url_shape_members_and_zero_result_try_emit_valid_go() {
    let go_http = emit(
        "import \"net/http\"\n\
         fn make_handler() http.handler {\n\
           return fn(w, r) {\n\
             r.parse_form()?\n\
             r.url.query.add(\"k\", \"v\")\n\
             r.url.query.del(\"old\")\n\
             w.header.del(\"X-Old\")\n\
             uri := r.request_uri\n\
             fv := r.form.get(\"k\")\n\
             ua := r.user_agent()\n\
             host := r.url.hostname()\n\
             port := r.url.port()\n\
             joined := r.url.join_path(\"v1\").string()\n\
             fmt.println(uri, fv, ua, host, port, joined)\n\
           }\n\
         }",
    );
    assert!(
        go_http.contains("hyErr0 := r.ParseForm()"),
        "expected zero-value error hoist `hyErr0 := r.ParseForm()`, got:\n{go_http}"
    );
    assert!(
        go_http.contains("r.URL.Query().Add(\"k\", \"v\")")
            && go_http.contains("r.URL.Query().Del(\"old\")")
            && go_http.contains("w.Header().Del(\"X-Old\")"),
        "expected Add/Del on url.Values and http.Header, got:\n{go_http}"
    );
    assert!(
        go_http.contains("uri := r.RequestURI")
            && go_http.contains("fv := r.Form.Get(\"k\")")
            && go_http.contains("ua := r.UserAgent()")
            && go_http.contains("host := r.URL.Hostname()")
            && go_http.contains("port := r.URL.Port()")
            && go_http.contains("joined := r.URL.JoinPath(\"v1\").String()"),
        "expected RequestURI, Form.Get, UserAgent, URL.Hostname, URL.Port, URL.JoinPath.String, got:\n{go_http}"
    );
}

#[test]
fn curated_sync_wait_group_once_and_strings_builder_fields_and_vars_emit_go_methods() {
    let go = emit(
        "import \"sync\"\n\
         import \"strings\"\n\
         struct worker {\n\
           wg sync.wait_group\n\
           once sync.once\n\
           buf strings.builder\n\
         }\n\
         fn run(w *worker) error {\n\
           w.wg.add(1)\n\
           w.wg.done()\n\
           w.wg.wait()\n\
           w.once.do(fn() {})\n\
           w.buf.grow(16)\n\
           w.buf.write_string(\"ok\")\n\
           var b strings.builder\n\
           b.write_byte(byte(10))?\n\
           fmt.println(w.buf.len(), w.buf.string(), b.string())\n\
           w.buf.reset()\n\
           return nil\n\
         }",
    );
    assert!(
        go.contains("w.wg.Add(1)") && go.contains("w.wg.Done()") && go.contains("w.wg.Wait()"),
        "expected WaitGroup Add/Done/Wait on struct field, got:\n{go}"
    );
    assert!(
        go.contains("w.once.Do(func() {"),
        "expected sync.Once Do on struct field, got:\n{go}"
    );
    assert!(
        go.contains("w.buf.Grow(16)")
            && go.contains("w.buf.WriteString(\"ok\")")
            && go.contains("hyErr0 := b.WriteByte(byte(10))")
            && go.contains("w.buf.Len(), w.buf.String(), b.String()")
            && go.contains("w.buf.Reset()"),
        "expected strings.Builder methods and 0-value WriteByte? hoist, got:\n{go}"
    );
}
