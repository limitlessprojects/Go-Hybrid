// 06-http-server — stresses: full Go stdlib interop (net/http) from Hybrid,
// including handlers as closures and middleware as higher-order functions.
// TARGET SYNTAX. Expected output: the server's routed responses (test client
// drives it in-process).

import "net/http"

fn logging(next http.handler) http.handler {
    return fn(w http.response_writer, r *http.request) {   // closure over next
        fmt.eprintln("{r.method} {r.url.path}")
        next.serve_http(w, r)
    }
}

fn main() {
    mux := http.new_serve_mux()

    mux.handle("/hello", http.handler(fn(w, r) {
        fmt.fprintln(w, "hello from Hybrid")
    }))

    mux.handle("/add/", http.handler(fn(w, r) {
        a := strconv.atoi(r.url.query.get("a"))?
        b := strconv.atoi(r.url.query.get("b"))?
        fmt.fprintf(w, "{a}+{b}={a + b}\n")
    }))

    // in-process smoke client (the acceptance test drives these)
    for _, path in ["/hello", "/add/?a=40&b=2"] {
        code, body := httptest.get(mux, path)?
        fmt.println("{code} {body}")
    }
}
