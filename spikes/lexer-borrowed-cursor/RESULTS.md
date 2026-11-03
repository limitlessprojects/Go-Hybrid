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

## Observed results — 3 November 2026

Host: Apple M2 / macOS-26.5.2-arm64-arm-64bit-Mach-O, arm64,
Rust 1.99.0, release profile. Baseline `df0cae63` retains the original lexer;
candidate `eb432aa4` borrows `Chars`. Both measurement executables used the same
driver SHA256 `e7522be724ff6ef260946a10d269adcf5b53e33e740af7d200ff7516e1c3e264`.
The driver was subsequently rustfmt-formatted without a logic change.

Medians of three fresh-process observations; each process performs ten lexes:

| Input | Bytes | Baseline ms/lex | Candidate ms/lex | Baseline max RSS MiB | Candidate max RSS MiB |
| --- | ---: | ---: | ---: | ---: | ---: |
| ordinary | 27,808 | 0.353 | 0.370 | 2.23 | 2.14 |
| comments | 1,080,014 | 1.316 | 0.671 | 45.08 | 2.64 |
| unicode | 54,808 | 0.341 | 0.329 | 2.23 | 2.08 |

The comment-heavy repeated-lex case has lower observed process peak RSS and
time. The ordinary-source elapsed median is about 4.9% slower; Unicode-source
time is slightly lower. These short, single-machine observations do not
establish statistical significance or a universal speedup. Returned tokens and
strings still allocate: this removes the separate source-wide character vector.

Complete token/diagnostic Debug snapshots match byte-for-byte on all 175 tracked
`.hy` files plus three generated inputs. Two new public API tests pass for
Unicode block/line-comment lookahead, tab/character columns, a multiline raw
string and one EOF. No grammar, API or diagnostic change is intended.

[measurements.json](measurements.json) retains every timing/RSS observation and
input hash; [inputs.py](inputs.py) regenerates the bounded inputs. Build and keep
the example at both revisions; compare its `snapshot FILE` bytes for
`git ls-files '*.hy'` and the generated inputs. The fixed `input.hy` diagnostic
label avoids private path differences. Original full outputs are retained by
the contributor. Debug is compared on the same Rust version, not proposed as a
new stable serialization format.

Existing toolchain warning: rust-objcopy could not strip debug information
because its bundled libLLVM.dylib was unavailable. Cargo exited 0; both sides used
the same installed toolchain/profile, and original warnings/hashes are retained.

Workspace, corpus, differential, examples and hosted CI: pending at this commit.
No maintainer acceptance, independent human review, award or payment is claimed.
