# Spike `W-1931`: Reserve, Don't Commit — Demand-Paged Deep-Stack Pool (`#1931`)

Replaces eager 1 MiB committed deep-stack slots with `PROT_NONE` virtual reservation, 4 KiB page-granularity demand paging, and an unmapped bottom guard page, keeping synchronous `callDeep1..4` ABI intact while cutting `VmRSS` below 25% of eager commit.

## Artifacts
- `spikes/w1931-reserve-dont-commit-stack-pool/stack_pool_rss.py`: Demand-paged deep-stack pool simulator and `VmRSS` / guard-page verifier.
- `spikes/w1931-reserve-dont-commit-stack-pool/tests/test_w1931_stack_pool_rss.py`: 4 deterministic unit tests.
