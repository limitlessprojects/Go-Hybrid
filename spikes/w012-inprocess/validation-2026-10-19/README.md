# W-012 archived CI measurements — 2026-10-19

This directory preserves the original JSON measurements from [W-012 validation run 39195170609](https://github.com/limitlessprojects/Go-Hybrid/actions/runs/39195170609). The three JSON sample files are byte-for-byte copies extracted from that run's artifacts. Archiving them did not rerun the benchmark or produce new timing samples. These results describe the historical source and tested merge below; they do not validate later code changes.

## Provenance

| Item | Recorded value |
| --- | --- |
| Pull request | [#556](https://github.com/limitlessprojects/Go-Hybrid/pull/556) |
| Source HEAD | `6dafd4973114fe35c77133db89c19828a0ad07b6` |
| Merge actually checked out by CI | `b49e0606be4746d90cb5399ebdb04166024676b5` |
| Base at that run | `aa4a9d91b5edecd2bff37eba23fdd7befe60bd56` |
| Measurement job | [analyzer-parity, 117876286217](https://github.com/limitlessprojects/Go-Hybrid/actions/runs/39195170609/job/117876286217) |
| Raw measurement timestamps | 2026-10-19 03:34:19–03:34:20 UTC |
| Host fields emitted by the harness | Linux, amd64, Go 1.26.9; 4 visible CPUs; `GOMAXPROCS=4` |

The artifact API identifies the source HEAD; the checkout log identifies the tested merge and its base. Those three commits are distinct. [manifest.json](manifest.json) records the original artifact URLs, sizes, expiration times, archive SHA-256 values, and SHA-256 values of the extracted files. Both downloaded ZIP digests were checked against GitHub's artifact metadata before extraction. The sample files contain synthetic filenames, hashes, measurements, environment fields, and limitation text; inspection found no credentials or private filesystem paths.

The original artifacts expire on **2026-10-26**, before the maintainer's [2026-11-01 M1 review window](https://github.com/limitlessprojects/Go-Hybrid/issues/268#issuecomment-6229962219). Keeping these raw files in the repository preserves their reviewability beyond that artifact expiration.

## Files and observed results

| File | Recorded workload | Purpose |
| --- | --- | --- |
| [w012-edit-benchmark.json](w012-edit-benchmark.json) | 25 files, 100 edits, warmup 10; 100 records with three path timings each | Warm request comparison and separate cold initialization |
| [w012-cold-cache-first.json](w012-cold-cache-first.json) | 2 files, 2 edits, warmup 1 | First Go process using the optional compilation-cache directory |
| [w012-cold-cache-second.json](w012-cold-cache-second.json) | 2 files, 2 edits, warmup 1 | A second Go process reusing that directory |

For the 100-edit run, the recorded summary fields are:

| Request path | p50 (ms) | p95 (ms) | p99 (ms) |
| --- | ---: | ---: | ---: |
| Native subprocess plus decode | 1.150408 | 1.282607 | 1.369949 |
| Public `Engine.Check` | 2.096157 | 2.777000 | 3.084019 |
| Experimental sequential reused guest | 1.450742 | 1.581058 | 1.718235 |

The public WASM path was slower than the native path on this host. Its separately recorded cold initialization was **358.947876 ms**. The two-process compilation-cache experiment recorded public `GetEngine` cold initialization of **395.981092 ms** in the first process and **12.217370 ms** in the second. These two cache runs have only two request samples each; they are not substitutes for the 100-edit comparison. All three files record matching WASM and native binary hashes.

## Interpretation limits

The timer starts after each file write. Each request checks only the edited file. The native path includes a new subprocess, reading the file, and decoding its envelope; the embedded path receives source bytes already in memory. This is an after-write request proxy, with no whole-workspace reanalysis, unsaved-buffer integration, gopls/LSP, or editor keystroke-to-diagnostic measurement.

The reused guest is a sequential experiment, not the public instance strategy or a production pool. Its cancellation, concurrency, and lifetime safety remain unproven. The compilation-cache result concerns cold initialization and does not establish faster hot checks. One fixture on one hosted Linux machine cannot establish population-level latency, production memory behavior, a Docker footprint reduction, or an M1 award.

The raw files retain their emitted `Limitations` strings unchanged. In the two-file cache runs, the generic string referring to “all 25 workspace files” is inherited harness wording; the authoritative `Files` field is **2**. The earlier [2026-10-18 snapshot](../validation-2026-10-18/README.md) remains a separate historical experiment.
