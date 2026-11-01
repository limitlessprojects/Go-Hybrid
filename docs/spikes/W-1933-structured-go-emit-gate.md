# Spike `W-1933`: Structured Go AST Emission + Self-Validation Gate (`#1933`)

Replaces raw string-concatenation templates in `hy-emit` with a typed Go AST builder, deterministic printer with `//line` source-span mapping, and pre-write syntax/scope validation gate (catching Row-13 BUILDABLE bugs like `#1769` `select` arm shadowing before `go build`).

## Artifacts
- `spikes/w1933-structured-go-emit-gate/structured_emit_gate.py`: Structured Go AST emitter and pre-write syntax/scope validator.
- `spikes/w1933-structured-go-emit-gate/tests/test_w1933_structured_emit_gate.py`: 4 deterministic unit tests.
