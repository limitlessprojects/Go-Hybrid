# Spike Results: W-4700 (Issue #4700)

## Summary
Milestone 1 reference model, binary serialization schema (`hyir` v0), and `go/ast` materializer for Wildcard Track issue #4700:
`[wildcard] Delete the Go-text emitter: checked graph lowers to a binary IR materialized as go/ast`.

The spike breaks the assumption that the compiler must serialize checked task graphs into plain Go text and re-parse them with `go/parser`. Lowering to a typed binary intermediate representation (`hyir`) completely eliminates the proof-to-artifact gap, rendering text-escaping, character encoding (such as the U+FEFF raw-string literal bug in #4666/#4670), and line directive misalignment bugs unrepresentable by design.

## Verification Gates
- **G1: Binary IR Serialization Round-trip**: Checked graphs serialize into compact length-prefixed `hyir` v0 binary payloads with 100% type and span fidelity.
- **G2: Elimination of Text Escaping Hazards**: String payloads with arbitrary Unicode control characters, null bytes, and Byte Order Marks (U+FEFF) pass safely across the boundary without string escaping crashes.
- **G3: Effect Certificate Enforcement**: Effect certificates are carried as first-class structured data. Any uncertified effect node is rejected with diagnostic `HYB-E6001: Uncertified effect node in hyir payload`.
- **G4: Materialization to go/ast**: Typed `hyir` entities materialize directly into Go AST constructs ready for native `go/types` verification.

## Conformance Matrix Results (4/4 Passed)
| Test Case | Description | Expected Diagnostic / Behavior | Result |
|:---|:---|:---:|:---:|
| `test_hyir_binary_serialization_roundtrip` | Full binary round-trip of module, tasks, and spans | 100% Fidelity | PASS |
| `test_materialization_into_go_ast` | Materialization to AST with certified effect tracking | Valid AST Structure | PASS |
| `test_falsifier_ufeff_and_raw_literal_hazard_eliminated` | Injection of U+FEFF and control bytes across boundary | Safe length-prefixed byte transfer | PASS |
| `test_falsifier_uncertified_effect_rejected` | Effect node without compiler verification certificate | `HYB-E6001` Rejection | PASS |
