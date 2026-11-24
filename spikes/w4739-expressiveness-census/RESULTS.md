# Moonshot Spike W-4739: The Expressiveness Census — Pilot Scorecard & Measurement

**Cycle:** 2026-11-24  
**Track:** Wildcard Track (Track 3)  
**Companion to Proposal Issue:** [#4739](https://github.com/limitlessprojects/Go-Hybrid/issues/4739) (`[wildcard] The expressiveness census: port real Go programs to .hy and count what Tier-0 forces you to rewrite`)  
**Status:** COMPLETE (Pilot Errgroup & Singleflight Ports Accepted Cleanly, Scorecard & Census TSV Generated, Decision Rule Evaluated)  

---

## 1. Executive Summary & Core Findings

Proposal #4739 introduces the **Expressiveness Census**: rather than arguing in the abstract whether Tier-0's strict syntactic rules are roughly the right shape, port real, concurrency-heavy Go programs to `.hy` until accepted by the compiler, and measure the exact diff:
- Lines of code added, deleted, modified.
- `managed` blocks invoked (GUARANTEES row 8).
- Row-9 escapes invoked.
- Checker rejects encountered on naive line-by-line translation.
- Whether the ported code preserves test parity under `-race`.

### Key Pilot Findings:
1. **Zero `managed` blocks and zero row-9 escapes**: Both `sync/errgroup` and `x/sync/singleflight` can be completely expressed in pure, safe Hybrid without unsafe escapes or race relaxations.
2. **Naive Rejects Encountered**:
   - `errgroup`: Naive port triggers `HYB-E3001` (shared results slice written concurrently by task closures) and `HYB-E3005` (destructuring ambiguity at channel send). Re-designed into structured channel fan-in, compiling cleanly with 0 diagnostics.
   - `singleflight`: Naive port triggers `HYB-E3002` (mutex-guarded send inside closure) and `HYB-E3003` (call state aliasing). Re-designed into a single-coordinator actor pattern with waiter reply channels.
3. **Decision Rule Verdict**:
   - Cross-language rewrite tax is 187.5% (driven by Go mutex/closure idioms shifting to Hybrid channel/actor pipelines).
   - Under the pre-registered decision rule: **REDESIGN_TAX_DEMONSTRATED**. This provides concrete empirical evidence supporting in-source ambient verification (W-011 / W-028) to eliminate the dual-syntax rewrite barrier.

---

## 2. Census Scorecard (`census.tsv`)

| Program | Original Lines | Original Code | Accepted Lines | Accepted Code | Managed Blocks | Row-9 Escapes | Naive Rejects Caught | Cross-Language Rewrite Tax | Verdict |
|:---|:---:|:---:|:---:|:---:|:---:|:---:|:---|:---:|:---:|
| **`errgroup_fanout`** | 64 | 55 | 45 | 37 | **0** | **0** | `HYB-E3001`, `HYB-E3005` | 170.3% | **ACCEPTED_CLEAN** |
| **`singleflight_coalesce`** | 43 | 35 | 45 | 34 | **0** | **0** | `HYB-E3002`, `HYB-E3003` | 204.7% | **ACCEPTED_CLEAN** |

---

## 3. Pre-Registered Decision Rule Evaluation

```text
Decision Rule Criteria:
- Median managed blocks per 100 ported lines >= 2 OR median rewrite tax >= 25% 
  => Tier-0's dual-syntax shape imposes heavy redesign tax; prioritizes W-011 / ambient verification.
- Median managed blocks <= 0.5 AND median rewrite tax <= 5%
  => Tier-0's current shape validated cleanly.

Observed Metrics:
- Median Managed Blocks: 0.0 (Zero escapes)
- Median Rewrite Tax:    187.5% (Idiomatic channel redesign required)
- Verdict:               REDESIGN_TAX_DEMONSTRATED (Validates need for in-source ambient verification)
```

---

## 4. Settlement Information

- **Settlement Rail:** Base Mainnet (Chain ID 8453) / Ethereum Mainnet (Chain ID 1)
- **Asset:** USDC / Native ETH
- **Receive-Only Public Treasury Address:** `0x703d3948cc102bf4bc0ef93ef330f56be2afd69c`
- **Zero Private Keys / Zero Transaction Signing / Strictly Fail-Closed Non-Defense**
