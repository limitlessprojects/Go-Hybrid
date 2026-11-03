# Borrowed lexer cursor — pre-registered comparison

Base: `b803f5556f4f75e8400bffe137f30717ae2e7831`.

Hypothesis: replacing the lexer-wide `Vec<char>` with a borrowed UTF-8 cursor
removes the full-source character copy without changing tokens, diagnostics,
or one-based character line/column positions. No grammar/API change is intended.
The token output still owns its strings. This is a normal memory/performance
improvement, with no new soundness/security finding.

Before changing the cursor, retain a release baseline executable built with the
same example driver. Generate the three bounded normal-source inputs with
`python3 spikes/lexer-borrowed-cursor/inputs.py NEW_DIRECTORY`.

Use `cargo build -p hy-syntax --example lexer_cursor --release --locked --offline`
from `compiler`. Run `lexer_cursor bench FILE 10` three times per input under
macOS `/usr/bin/time -l`; retain duration and maximum resident-set-size outputs.
Read input outside the timed loop. Every invocation is a fresh process; inputs
are already written, so this is not a cold-cache measurement. RSS includes the
process, input, and token output, not just the cursor allocation.

Compare complete `snapshot FILE` bytes between baseline and candidate for all
tracked `.hy` files and the three generated inputs. Also run existing syntax and
workspace/corpus/differential/example checks. Snapshot formatting is a same-Rust-
version comparison, not a public serialization format. No library dependencies
are added. Report any mismatches or material time regression; no speedup is
assumed in advance and a resource-only improvement must state its tradeoff.

Results: not run at this commit. Existing PRs and their source remain unchanged.
