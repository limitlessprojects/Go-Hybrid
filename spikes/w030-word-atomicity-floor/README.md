# Spike W-030 (`#1210`): Word-Atomicity Floor Below Tier-0 (`ACCEPT-MANAGED`)

Author: `@planners1490` (original author of Wildcard proposal `#1210`).

## Purpose & Pre-Registered Falsifiers (`F0`–`F4`)

`SPEC-LANGUAGE.md` §9.5 (`O6.1`), §9.9 (`O20`), and `GUARANTEES.md` row 17 state that `managed(...)` waives race-freedom only and is **never** a memory-safety escape. Under Go's memory model, racy writes to multi-word values (`[]T` 3-word slice headers, `string` 2-word headers, `any`/`interface{}` 2-word headers) tear across machine words and cause out-of-bounds memory access or pointer/integer type confusion.

This executable spike packages:
1. **Real `.hy` / `.go` reproducers (`torn.hy`, `torn_write.hy`, `confuse.hy`) and emitted Go (`torn.bin.go`, `confuse.bin.go`)** audited against `hyb` (`ACCEPT-MANAGED`, exit 0).
2. **Word-width reachability classifier (`O4.6`) & `atomic.Pointer[T]` check-site lowering** (`--managed-word-atomic`) at `HYB-E8001` boundaries.
3. **Live Go benchmark & execution harness (`box_test.go`)** plus corpus zero-regression audit (`0` emitted lines changed across all shipped `ACCEPT-MANAGED` corpus fixtures and examples).

## Run

```bash
python3 spikes/w030-word-atomicity-floor/word_atomicity_floor.py --check
python3 -m unittest discover -s spikes/w030-word-atomicity-floor/tests -p "test_*.py" -v
```
