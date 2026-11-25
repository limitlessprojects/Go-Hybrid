# W-2620 Spike — Compiler-Generated Durable Continuations

- **Issue:** #2620 (`[wildcard] Let a task outlive its process: compiler-generated durable continuations`)
- **Spike directory:** `spikes/w-durable-continuations-2620/`
- **Verifier CLI:** `python3 spikes/w-durable-continuations-2620/durable_continuations_verifier.py --check --live-go`
- **Test suites:** `pytest spikes/w-durable-continuations-2620/tests/test_w2620_spike.py` & `cd spikes/w-durable-continuations-2620/work && go test -v ./...`
- **Summary:** Proves that an opt-in closed task region lowered to versioned continuation state machines with explicit PCs, serializable value locals, bounded FIFO channel queues, and atomic consistent-cut storage commits recovers deterministically across process failures at any transition boundary without re-executing completed prefixes, while rejecting unsupported constructs (`HYB-E3620`) and catching all 5 negative-control snapshot corruptions (`F1`–`F5`).
