# Row-13 sample: Hybrid inside an EXISTING go.mod project

The 10-minute path from [discussion #3](https://github.com/limitlessprojects/Go-Hybrid/discussions/3) (design B: `go:generate`, checked-in generated Go, `--check` CI mode). This directory IS the sample — an ordinary Go module that added two Hybrid files:

    go.mod                       ordinary module + the two hybrid.program lines
    app.go                       hand-written Go, unchanged
    calc.hy                      the Hybrid-checked file (ownership-checked concurrent sum)
    calc.hy.go                   GENERATED Go (checked in, reviewable, diffable)

## The 10 minutes, for your project

1. Install Hybrid (see the release page's one-command install), then vendor the runtime once:

       hyb vendor-runtime .

   This creates `./hybridrt/` (the binding packages + `lib/libhyrt.a` + its own
   `module hybrid.program` go.mod).

2. Add two lines to your `go.mod`:

       require hybrid.program v0.0.0
       replace hybrid.program => ./hybridrt

3. Drop `.hy` files beside your `.go` files, and put ONE generate line in any Go file of the package:

       //go:generate hyb generate .

4. Run the ordinary build:

       go generate ./...
       go build ./... && go run . && go test ./...

`calc.hy.go` is plain Go: read it, diff it, review it. **Delete `calc.hy` and the
project is a plain Go module again** — the pitch is literal.

## CI (staleness is a build failure)

    hyb generate --check . && go build ./...

A `.hy` edited without regenerating fails `--check` (exit 1) — an untranspiled
file can never silently skip the checker.

## Bridge code (rust/ imports) works the same way

The vendored runtime ships `lib/libhyrt.a`; `hyb vendor-runtime .` copies the
fresh arch-matched staticlib. A `.hy` file with `import "rust/serde_json"`
transpiles to cgo-using Go that builds under the same `go build ./...`.
