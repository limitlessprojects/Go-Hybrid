// GoBench moby/22941. The parser has no `interface` decl, so the Conn interface
// collapses to the concrete *pipe: the transliteration is already lossy.
struct pipe {
    wr_mu int
}

fn (p *pipe) write(b []byte) {
    _ = b
}

fn push(p *pipe, b []byte) {
    p.write(b)
}

fn main() {
    p := pipe{}
    task push(&p, []byte("GET /foo"))
}