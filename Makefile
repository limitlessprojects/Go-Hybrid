# Go-Hybrid — one-command routine actions (SPEC.md §14.1)
# NOTE: `just` was deferred (PLAN.md Decision Log); make is ubiquitous on this box.
# RECIPES ARE TAB-INDENTED (8639e0b fixed this once; Batch I's regeneration
# silently reintroduced spaces — make check now guards the syntax, see below).
SHELL := /bin/sh
PY := python3
KAGGLE_CLI ?= /home/z/.venv/bin/kaggle
JOB ?= 001-spike0-kaggle-smoke
SHARDS ?= 4
MAXPAR ?= 4

CARGO := $(HOME)/.cargo/bin/cargo
CARGO_TARGET_DIR ?= $(HOME)/go-hybrid-target
HYB_BIN := $(CARGO_TARGET_DIR)/debug/hyb

.PHONY: help env check test-quick ci corpus workspace examples differential bench spike0-kaggle spike0-kaggle-status spike0-kaggle-fetch \
        spike0-actions-4 spike0-actions-20 spike-1 spike-2 spike-3 kaggle-job kaggle-status kaggle-fetch kaggle-wait \
        actions-dispatch actions-watch census clean

help:
	@echo "make env              — verify pinned toolchains (source tools/env.sh)"
	@echo "make check            — quick sanity: tool syntax + runner selftest + hygiene"
	@echo "make test-quick       — sandbox unit tests (grows with the compiler)"
	@echo "make ci               — what CI runs on push"
	@echo "make examples         — the ten target programs, byte-exact acceptance"
	@echo "make corpus           — Tier-0 corpus verdicts (115/115)"
	@echo "make census           — every stated corpus count matches the corpus"
	@echo "make differential     — the differential corpus loop (accept + reject)"
	@echo "make bench            — G2 call-overhead bench on the live bridge (hyb bench-overhead)"
	@echo "make spike0-kaggle    — submit Spike 0 part A to the Kaggle worker"
	@echo "make spike0-actions-4 / -20  — dispatch the Actions fleet smoke test"
	@echo "make spike-1 / spike-2 / spike-3 — re-run that spike's bench suite (sandbox)"
	@echo "make kaggle-job J=NNN-name   — submit jobs/\$(J) to the worker"
	@echo "make clean            — prune caches (disk hygiene)"

env:
	@tools/env.sh

check:
	@$(MAKE) -n examples >/dev/null 2>&1 || { echo "MAKE SYNTAX BROKEN (recipes must be TAB-indented)"; exit 1; }
	@echo "make-syntax OK"
	@$(PY) -m py_compile tools/runner/runner.py tools/job.py worker/script.py tools/corpus_harness.py tools/differential_loop.py && echo "py-syntax OK"

	@$(PY) -m unittest discover -s tools -p test_differential_loop.py -v
	@$(PY) -m unittest discover -s tools -p test_check_versions.py -v
	@$(PY) -m unittest discover -s tools -p test_workload_run.py -v
	@$(PY) -m unittest discover -s tools -p test_runner_env_report.py -v
	@$(PY) -m unittest discover -s tools -p test_corpus_harness.py -v
	@$(PY) -m unittest discover -s tools -p test_package_version.py -v
	@$(PY) -m unittest discover -s tools/runner -p test_runner.py -v
	@$(PY) -m unittest discover -s tools -p test_check_test_registration.py -v
	@$(PY) -m unittest discover -s tools -p test_check_corpus_census.py -v
	@$(PY) tools/check_corpus_census.py && echo "corpus census OK"
	@$(PY) tools/check_test_registration.py && echo "test registration OK"
	@bash tools/check-versions.sh --no-toolchain && echo "versions OK"
	@$(PY) tools/check_conflict_markers.py && echo "conflict markers OK"
	@cd /tmp && rm -rf gh-selftest && mkdir gh-selftest && cd gh-selftest && \
	printf '%s\n' '{"job":"selftest","tier":"sandbox","sha":"HEAD","repo":"none","gpu":false,"setup":[],"commands":[{"name":"echo","run":"echo selftest-ok","timeout":30}],"expected_results":[]}' > job.json && \
	$(PY) $(CURDIR)/tools/runner/runner.py job.json --results-dir results >/dev/null && \
	test -f results/summary.json && grep -q '"ok": true' results/summary.json && echo "runner selftest OK"
	@$(PY) -c "import glob,json;[json.load(open(p)) for p in glob.glob('jobs/*/job.json')];print('job specs OK')"
	@! git grep -qE 'ghp_[A-Za-z0-9]{20,}|KGAT_[A-Za-z0-9]{20,}|eyJ[A-Za-z0-9_-]{20,}\.[A-Za-z0-9_-]{10,}' HEAD 2>/dev/null || (echo "SECRET LEAK"; exit 1)
	@cd compiler && CARGO_TARGET_DIR=$(CARGO_TARGET_DIR) CARGO_BUILD_JOBS=2 $(CARGO) check --workspace --quiet && echo "cargo check OK"
	@echo "check OK"

test-quick: check
	@cd compiler && CARGO_TARGET_DIR=$(CARGO_TARGET_DIR) CARGO_BUILD_JOBS=2 $(CARGO) test --workspace --quiet
	@$(PY) tools/corpus_harness.py --bin $(HYB_BIN)
	@$(PY) tools/differential_loop.py --bin $(HYB_BIN)

# MVP-0 walking-skeleton acceptance: parse → Tier-0 check → emit → go build
# → run → diff against each example's expected.txt.
examples:
	@cd compiler && CARGO_TARGET_DIR=$(CARGO_TARGET_DIR) CARGO_BUILD_JOBS=2 $(CARGO) build -p hyb --quiet
	@bash tools/run_examples.sh

workspace:
	@cd compiler && CARGO_TARGET_DIR=$(CARGO_TARGET_DIR) CARGO_BUILD_JOBS=2 $(CARGO) check --workspace

corpus:
	@cd compiler && CARGO_TARGET_DIR=$(CARGO_TARGET_DIR) CARGO_BUILD_JOBS=2 $(CARGO) build -p hyb --quiet
	@$(PY) tools/corpus_harness.py --bin $(HYB_BIN)

census:
	@$(PY) tools/check_corpus_census.py

# The differential corpus loop (Batch I): accepted corpus programs must
# survive the full pipeline (hyb build + run, exit 0); rejected programs
# must be rejected by the checker. SPEC §"Differential tests on the corpus".
differential:
	@cd compiler && CARGO_TARGET_DIR=$(CARGO_TARGET_DIR) CARGO_BUILD_JOBS=2 $(CARGO) build -p hyb --quiet
	@$(PY) tools/differential_loop.py --bin $(HYB_BIN)

# The G2 gate's call-overhead bench (Batch J): assembles the bench through
# the exact hyb workspace recipe and measures the no-op crossing on the live
# bridge. Two scenarios (shallow / grown stack); median of 3 process runs.
bench:
	@cd compiler && CARGO_TARGET_DIR=$(CARGO_TARGET_DIR) CARGO_BUILD_JOBS=2 $(CARGO) build -p hyb --quiet
	@$(HYB_BIN) bench-overhead

ci: check
	@echo "CI parity with .github/workflows/ci.yml"

# ---- Spike 0 orchestration ----
spike0-kaggle:
	$(PY) tools/job.py kaggle submit jobs/$(JOB)

spike0-kaggle-status:
	$(PY) tools/job.py kaggle status

spike0-kaggle-fetch:
	$(PY) tools/job.py kaggle fetch jobs/$(JOB)

spike0-actions-4:
	$(PY) tools/job.py actions dispatch heavy "$(shell git rev-parse --short=40 HEAD)" $(JOB) 4 4

spike0-actions-20:
	$(PY) tools/job.py actions dispatch heavy "$(shell git rev-parse --short=40 HEAD)" $(JOB) 20 20

kaggle-job:
	$(PY) tools/job.py kaggle submit jobs/$(J)

kaggle-status:
	$(PY) tools/job.py kaggle status

kaggle-wait:
	$(PY) tools/job.py kaggle wait $(or $(MINUTES),20)

kaggle-fetch:
	$(PY) tools/job.py kaggle fetch jobs/$(J)

spike-1:
	bash spikes/spike1-call-overhead/work/run.sh

spike-2:
	bash spikes/spike2-go-linking/work/run.sh

spike-3:
	bash spikes/spike3-memory/work/run.sh

spike-5:
	bash spikes/spike5-errors/work/run.sh

spike-4:
	bash spikes/spike4-scheduling/work/run.sh

actions-watch:
	$(PY) tools/job.py actions watch

clean:
	@command -v cargo >/dev/null && cargo clean 2>/dev/null || true
	@if command -v go >/dev/null; then GOFLAGS= go clean -cache 2>/dev/null || true; fi
	@df -h / | tail -1
