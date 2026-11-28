"""W-4851 (#4851): Compile Serial Modular Recurrences into Ordered Chunk
Transformers -- spike & verifier for jimmy1008's wildcard proposal.

    python3 chunk_transformer.py --check [--json] [--live-go]

unit tests:

    python3 -m unittest discover -s spikes/w4851-ordered-chunk-transformers/tests -p "test_*.py" -v

The proposal: a serial `s = A(input[i])*s + B(input[i]) mod 2**w` loop, with
immutable random-access input and total, effect-free, input-only
coefficients, can be lowered to contiguous chunk tasks. Each chunk reduces
to an (A, B) pair describing `s -> A*s+B`; chunks may finish in any order,
but the collector must compose their summaries in SOURCE order (composition
is associative, not commutative). This spike adds the two pieces the
original submission explicitly left open: a checked syntactic extractor
over literal `.hy`-shaped source (admits the narrow form, refuses every
excluded shape) and live Go code generation so the extracted expressions
are executed, not just modeled in Python.

Pre-registered hypotheses & falsifiers:

P1: the extractor admits the narrow affine-recurrence loop shape with
    input-only, call-free, non-float coefficients, and REFUSES every
    excluded shape (state-dependent coefficient, opaque/effectful call,
    unrecognized loop body) -- each refusal falls back to the serial loop.
P2: for every admitted program, summarizing contiguous chunks as (A, B)
    and composing the summaries IN SOURCE ORDER reproduces the serial
    reference evaluator's final state exactly, across random word widths
    (8/16/32/64-bit), lengths, chunk counts, coefficient functions and
    initial states.
P3 (falsifier -- must FIRE): composing the same chunk summaries in a
    non-source order (the completion-order bug the proposal warns against)
    diverges from the serial reference on the large majority of cases.
P4 (falsifier -- must FIRE): a genuinely state-dependent recurrence
    (`s -> s*s`) cannot be admitted by probing two points and pretending
    the result is affine -- the two-point probe's prediction diverges from
    the true recurrence at a third point, for every modulus tried.
P5 (--live-go only): Go source generated from one admitted extraction --
    a serial function and an ordered-chunk-transformer built from real
    goroutines, both reusing the extractor's literal coefficient
    expressions verbatim -- builds, runs, and matches byte-for-byte across
    randomized inputs, lengths, chunk counts and initial states; a
    reversed-order collector, generated and run the same way, diverges.
"""
import argparse
import json
import pathlib
import random
import re
import shutil
import subprocess
import sys
import tempfile
from dataclasses import dataclass
from typing import Callable, List, Optional, Sequence, Tuple

SPIKE_DIR = pathlib.Path(__file__).resolve().parent

WORD_BITS = {"uint8": 8, "uint16": 16, "uint32": 32, "uint64": 64}

# A checked syntactic extractor over literal `.hy`-shaped source: recognizes
# exactly the admitted loop shape and captures the two coefficient
# sub-expressions verbatim, so codegen never re-derives what the extractor
# already proved -- the "exact extraction" risk the proposal names.
_LOOP_RE = re.compile(
    r"fn\s+(?P<name>\w+)\s*\(\s*input\s+\[\]\s*(?P<width>uint8|uint16|uint32|uint64)\s*,\s*s0\s+(?P=width)\s*\)\s*(?P=width)\s*\{\s*"
    r"s\s*:=\s*s0\s*"
    r"for\s+i\s+in\s+0\.\.len\(input\)\s*\{\s*"
    r"s\s*=\s*(?P<a_expr>.+?)\s*\*\s*s\s*\+\s*(?P<b_expr>.+?)\s*"
    r"\}\s*"
    r"return\s+s\s*\}",
    re.DOTALL,
)
_STATE_TOKEN_RE = re.compile(r"\bs\b")
_CALL_RE = re.compile(r"\b[A-Za-z_]\w*\s*\(")
_ALLOWED_CHARS_RE = re.compile(r"^[0-9A-Za-z_\[\]\.\+\-\*\(\)\s]+$")


@dataclass(frozen=True)
class AffineLoopSpec:
    name: str
    width_bits: int
    a_expr: str
    b_expr: str

    @property
    def modulus(self) -> int:
        return 1 << self.width_bits


@dataclass(frozen=True)
class ExtractionResult:
    spec: Optional[AffineLoopSpec]
    admitted: bool
    reason: str


def extract_affine_loop(hy_source: str) -> ExtractionResult:
    """Checked syntactic extraction (admission control for the backend).

    Admits `s = A(input[i])*s + B(input[i])` only when A and B are built
    from `input[i]`, integer literals and `+ - * ( )` -- no reference to
    the loop state `s`, no function call (so no unproven effect or
    non-totality), no unrecognized syntax. Everything else refuses, which
    is the documented fallback-to-serial path, not a crash.
    """
    match = _LOOP_RE.search(hy_source)
    if not match:
        return ExtractionResult(None, False, "loop shape not recognized; falling back to the serial loop")

    a_expr, b_expr = match.group("a_expr").strip(), match.group("b_expr").strip()
    for label, expr in (("coefficient A", a_expr), ("coefficient B", b_expr)):
        if _STATE_TOKEN_RE.search(expr):
            return ExtractionResult(None, False, f"{label} references loop state s; state-dependent recurrence refused")
        if _CALL_RE.search(expr):
            return ExtractionResult(None, False, f"{label} calls a function; effect/totality cannot be proven, refused")
        if not _ALLOWED_CHARS_RE.match(expr):
            return ExtractionResult(None, False, f"{label} contains unrecognized syntax; refused")

    spec = AffineLoopSpec(match.group("name"), WORD_BITS[match.group("width")], a_expr, b_expr)
    return ExtractionResult(spec, True, "admitted: state-independent, call-free affine coefficients")


# --- the chunk-transformer algebra (s -> A*s+B mod 2**w) -------------------

IDENTITY = (1, 0)


def compose(left: Tuple[int, int], right: Tuple[int, int], modulus: int) -> Tuple[int, int]:
    """Left then right: associative, not commutative."""
    a_l, b_l = left
    a_r, b_r = right
    return (a_r * a_l) % modulus, (a_r * b_l + b_r) % modulus


def apply_summary(summary: Tuple[int, int], state: int, modulus: int) -> int:
    a, b = summary
    return (a * state + b) % modulus


def serial_reference(digits: Sequence[int], state0: int, a_fn: Callable[[int], int], b_fn: Callable[[int], int], modulus: int) -> int:
    """The independent oracle: the literal serial loop, no chunking at all."""
    state = state0 % modulus
    for digit in digits:
        state = (a_fn(digit) * state + b_fn(digit)) % modulus
    return state


def summarize_chunk(chunk: Sequence[int], a_fn: Callable[[int], int], b_fn: Callable[[int], int], modulus: int) -> Tuple[int, int]:
    summary = IDENTITY
    for digit in chunk:
        summary = compose(summary, (a_fn(digit) % modulus, b_fn(digit) % modulus), modulus)
    return summary


# --- the cost gate: fall back to serial when splitting is inconclusive -----

MIN_CHUNK_SIZE = 64


def choose_chunk_count(n: int, requested_chunks: int) -> int:
    """O(n+k) work and O(n/k + log k) depth only pay off once chunks clear a
    minimum size; below that, splitting is cost-inconclusive and the gate
    collapses to a single chunk (the chunked path degenerates to serial).
    """
    if requested_chunks <= 1 or n < MIN_CHUNK_SIZE * 2:
        return 1
    return max(1, min(requested_chunks, n // MIN_CHUNK_SIZE))


def split_contiguous(digits: Sequence[int], requested_chunks: int) -> List[Sequence[int]]:
    n = len(digits)
    chunk_count = choose_chunk_count(n, requested_chunks)
    if chunk_count <= 1 or n == 0:
        return [digits]
    base, extra = divmod(n, chunk_count)
    chunks, start = [], 0
    for idx in range(chunk_count):
        size = base + (1 if idx < extra else 0)
        chunks.append(digits[start:start + size])
        start += size
    return [c for c in chunks if c]


def ordered_collect(chunk_summaries: Sequence[Tuple[int, int]], state0: int, modulus: int) -> int:
    """The collector: fold chunk summaries in SOURCE order, regardless of
    the order in which the chunks were computed."""
    acc = IDENTITY
    for summary in chunk_summaries:
        acc = compose(acc, summary, modulus)
    return apply_summary(acc, state0, modulus)


def completion_order_mutant_collect(chunk_summaries: Sequence[Tuple[int, int]], state0: int, modulus: int) -> int:
    """The bug the proposal warns against: combine using a non-source
    order (modeled here as the reverse of source order, the same mutant
    the original submission used in Python)."""
    return ordered_collect(list(reversed(list(chunk_summaries))), state0, modulus)


def compile_chunked(digits: Sequence[int], state0: int, a_fn: Callable[[int], int], b_fn: Callable[[int], int], modulus: int, requested_chunks: int, mutate_collector: bool = False) -> int:
    chunks = split_contiguous(digits, requested_chunks)
    summaries = [summarize_chunk(chunk, a_fn, b_fn, modulus) for chunk in chunks]
    collect = completion_order_mutant_collect if mutate_collector else ordered_collect
    return collect(summaries, state0, modulus)


# --- P4: why a state-dependent recurrence must not be admitted -------------

def two_point_affine_probe(recurrence: Callable[[int], int], modulus: int) -> Tuple[int, int]:
    """Pretend `recurrence` is affine in its own output: probe at 0 and 1,
    solve A*0+B = f(0), A*1+B = f(1) for (A, B)."""
    f0, f1 = recurrence(0) % modulus, recurrence(1) % modulus
    a = (f1 - f0) % modulus
    b = f0
    return a, b


def quadratic_admission_hazard(modulus: int, witness: int = 2) -> Tuple[int, int]:
    """Falsifier P4: `s -> s*s` probed at 0 and 1 predicts a value at
    `witness` that the true recurrence does not produce."""
    recurrence = lambda s: (s * s) % modulus
    a, b = two_point_affine_probe(recurrence, modulus)
    predicted = (a * witness + b) % modulus
    actual = recurrence(witness)
    return predicted, actual


# --- randomized P2/P3 verification (pure Python, no Go needed) -------------

def _random_coefficient_fns(rng: random.Random, modulus: int) -> Tuple[Callable[[int], int], Callable[[int], int]]:
    c0, c1, c2, c3 = (rng.randrange(modulus) for _ in range(4))
    return (lambda d: (c0 + c1 * d) % modulus), (lambda d: (c2 + c3 * d) % modulus)


def run_randomized_verification(seed: int = 0, trials: int = 400) -> dict:
    """n is kept above the cost gate's two-chunk floor so the requested
    chunk count is actually honored -- otherwise most random (n, k) pairs
    would collapse to a single chunk and the collector-order mutant would
    trivially agree with the correctly-ordered collector."""
    rng = random.Random(seed)
    ordered_matches = 0
    multi_chunk_trials = multi_chunk_mutant_matches = 0
    first_mutant_mismatch = None
    for _ in range(trials):
        width_bits = rng.choice(list(WORD_BITS.values()))
        modulus = 1 << width_bits
        n = rng.randrange(2 * MIN_CHUNK_SIZE, 2 * MIN_CHUNK_SIZE + 200)
        digits = [rng.randrange(0, 256) for _ in range(n)]
        state0 = rng.randrange(modulus)
        requested_chunks = rng.randrange(2, 9)
        a_fn, b_fn = _random_coefficient_fns(rng, modulus)

        expected = serial_reference(digits, state0, a_fn, b_fn, modulus)
        ordered = compile_chunked(digits, state0, a_fn, b_fn, modulus, requested_chunks, mutate_collector=False)
        mutant = compile_chunked(digits, state0, a_fn, b_fn, modulus, requested_chunks, mutate_collector=True)

        ordered_matches += ordered == expected
        chunk_count = choose_chunk_count(n, requested_chunks)
        if chunk_count > 1:
            multi_chunk_trials += 1
            if mutant == expected:
                multi_chunk_mutant_matches += 1
            elif first_mutant_mismatch is None:
                first_mutant_mismatch = {
                    "width_bits": width_bits, "n": n, "chunk_count": chunk_count,
                    "state0": state0, "expected": expected, "mutant": mutant,
                }
    return {
        "trials": trials,
        "ordered_matches": ordered_matches,
        "ordered_all_match": ordered_matches == trials,
        "multi_chunk_trials": multi_chunk_trials,
        "multi_chunk_mutant_matches": multi_chunk_mutant_matches,
        "multi_chunk_mutant_mismatches": multi_chunk_trials - multi_chunk_mutant_matches,
        "first_mutant_mismatch": first_mutant_mismatch,
    }


# --- P5: live Go code generation (optional, requires `go` on PATH) ---------

_GO_TEMPLATE = """package main

import (
	"bufio"
	"fmt"
	"os"
	"sort"
	"sync"
)

func serial(input []uint8, s0 uint8) uint8 {
	s := s0
	for i := 0; i < len(input); i++ {
		s = (%(a_expr)s) * s + (%(b_expr)s)
	}
	return s
}

func chunkSummary(input []uint8) (uint8, uint8) {
	a, b := uint8(1), uint8(0)
	for i := 0; i < len(input); i++ {
		ai := (%(a_expr)s)
		bi := (%(b_expr)s)
		a, b = ai*a, ai*b+bi
	}
	return a, b
}

func splitContiguous(input []uint8, chunkCount int) [][]uint8 {
	n := len(input)
	if chunkCount <= 1 || n == 0 {
		return [][]uint8{input}
	}
	if chunkCount > n {
		chunkCount = n
	}
	base, extra := n/chunkCount, n%%chunkCount
	chunks := make([][]uint8, 0, chunkCount)
	start := 0
	for i := 0; i < chunkCount; i++ {
		size := base
		if i < extra {
			size++
		}
		if size == 0 {
			continue
		}
		chunks = append(chunks, input[start:start+size])
		start += size
	}
	return chunks
}

// chunkedOrdered: chunks run as goroutines that may finish in any order,
// but each writes its summary into its SOURCE-indexed slot, so the final
// fold below always combines in source order.
func chunkedOrdered(input []uint8, s0 uint8, chunkCount int) uint8 {
	chunks := splitContiguous(input, chunkCount)
	summaries := make([][2]uint8, len(chunks))
	var wg sync.WaitGroup
	order := rand_permutation(len(chunks))
	for slot, idx := range order {
		wg.Add(1)
		go func(slot, idx int) {
			defer wg.Done()
			a, b := chunkSummary(chunks[idx])
			summaries[idx] = [2]uint8{a, b}
			_ = slot
		}(slot, idx)
	}
	wg.Wait()
	accA, accB := uint8(1), uint8(0)
	for _, sm := range summaries {
		accA, accB = sm[0]*accA, sm[0]*accB+sm[1]
	}
	return accA*s0 + accB
}

// chunkedReversedMutant: the bug the proposal warns against -- combine the
// same per-chunk summaries in a non-source order (here: reversed).
func chunkedReversedMutant(input []uint8, s0 uint8, chunkCount int) uint8 {
	chunks := splitContiguous(input, chunkCount)
	summaries := make([][2]uint8, len(chunks))
	var wg sync.WaitGroup
	for idx := range chunks {
		wg.Add(1)
		go func(idx int) {
			defer wg.Done()
			a, b := chunkSummary(chunks[idx])
			summaries[idx] = [2]uint8{a, b}
		}(idx)
	}
	wg.Wait()
	accA, accB := uint8(1), uint8(0)
	for i := len(summaries) - 1; i >= 0; i-- {
		sm := summaries[i]
		accA, accB = sm[0]*accA, sm[0]*accB+sm[1]
	}
	return accA*s0 + accB
}

func rand_permutation(n int) []int {
	order := make([]int, n)
	for i := range order {
		order[i] = i
	}
	sort.SliceStable(order, func(i, j int) bool { return (order[i]*2654435761)%%97 < (order[j]*2654435761)%%97 })
	return order
}

// stdin protocol (one line per case): width s0 chunkCount n d0 d1 ... d(n-1)
// stdout: one line per case: serial ordered reversedMutant
func main() {
	scanner := bufio.NewScanner(os.Stdin)
	scanner.Buffer(make([]byte, 1024*1024), 1024*1024)
	writer := bufio.NewWriter(os.Stdout)
	defer writer.Flush()
	for scanner.Scan() {
		line := scanner.Text()
		if line == "" {
			continue
		}
		var s0i, chunkCount, n int
		fields := splitFields(line)
		s0i = atoiMust(fields[0])
		chunkCount = atoiMust(fields[1])
		n = atoiMust(fields[2])
		input := make([]uint8, n)
		for i := 0; i < n; i++ {
			input[i] = uint8(atoiMust(fields[3+i]))
		}
		s0 := uint8(s0i)
		fmt.Fprintf(writer, "%%d %%d %%d\\n", serial(input, s0), chunkedOrdered(input, s0, chunkCount), chunkedReversedMutant(input, s0, chunkCount))
	}
}

func splitFields(line string) []string {
	var fields []string
	cur := ""
	for _, c := range line {
		if c == ' ' {
			if cur != "" {
				fields = append(fields, cur)
				cur = ""
			}
			continue
		}
		cur += string(c)
	}
	if cur != "" {
		fields = append(fields, cur)
	}
	return fields
}

func atoiMust(s string) int {
	n := 0
	for _, c := range s {
		n = n*10 + int(c-'0')
	}
	return n
}
"""


def render_go_source(spec: AffineLoopSpec) -> str:
    if spec.width_bits != 8:
        raise ValueError("the live-Go harness below is written for uint8 only")
    return _GO_TEMPLATE % {"a_expr": spec.a_expr, "b_expr": spec.b_expr}


def run_live_go_verification(spec: AffineLoopSpec, seed: int = 0, trials: int = 60) -> dict:
    if not shutil.which("go"):
        return {"skipped": True, "reason": "go not on PATH"}

    rng = random.Random(seed)
    cases = []
    for _ in range(trials):
        n = rng.randrange(0, 150)
        cases.append({
            "s0": rng.randrange(256),
            "chunk_count": rng.randrange(1, 9),
            "digits": [rng.randrange(256) for _ in range(n)],
        })

    with tempfile.TemporaryDirectory(prefix="w4851-go-") as td:
        tdp = pathlib.Path(td)
        (tdp / "go.mod").write_text("module w4851spike\n\ngo 1.21\n")
        (tdp / "main.go").write_text(render_go_source(spec))
        build = subprocess.run(["go", "build", "-o", str(tdp / "runner"), "."], cwd=td, capture_output=True, text=True, timeout=60)
        if build.returncode != 0:
            return {"skipped": False, "build_ok": False, "stderr": build.stderr}

        stdin_lines = []
        for case in cases:
            fields = [str(case["s0"]), str(case["chunk_count"]), str(len(case["digits"]))] + [str(d) for d in case["digits"]]
            stdin_lines.append(" ".join(fields))
        run = subprocess.run([str(tdp / "runner")], input="\n".join(stdin_lines) + "\n", capture_output=True, text=True, timeout=60)
        if run.returncode != 0:
            return {"skipped": False, "build_ok": True, "run_ok": False, "stderr": run.stderr}

        out_lines = [l for l in run.stdout.splitlines() if l.strip()]
        ordered_matches = mutant_matches = 0
        a_fn = lambda d: eval(spec.a_expr.replace("input[i]", "d"), {"d": d})  # noqa: S307 -- fixed, locally generated expression, not external input
        b_fn = lambda d: eval(spec.b_expr.replace("input[i]", "d"), {"d": d})  # noqa: S307
        for case, line in zip(cases, out_lines):
            serial_go, ordered_go, mutant_go = (int(x) for x in line.split())
            reference = serial_reference(case["digits"], case["s0"], a_fn, b_fn, 256)
            ordered_matches += (serial_go == reference and ordered_go == reference)
            mutant_matches += (mutant_go == reference)
        return {
            "skipped": False, "build_ok": True, "run_ok": True,
            "trials": len(cases),
            "ordered_all_match": ordered_matches == len(cases),
            "mutant_matches": mutant_matches,
            "mutant_mismatches": len(cases) - mutant_matches,
        }


# --- checks ------------------------------------------------------------

ADMITTED_HY = """
fn rolling(input []uint8, s0 uint8) uint8 {
    s := s0
    for i in 0..len(input) {
        s = (3+input[i])*s + input[i]
    }
    return s
}
"""

STATE_DEPENDENT_HY = """
fn broken(input []uint8, s0 uint8) uint8 {
    s := s0
    for i in 0..len(input) {
        s = (s+input[i])*s + input[i]
    }
    return s
}
"""

EFFECTFUL_CALL_HY = """
fn broken(input []uint8, s0 uint8) uint8 {
    s := s0
    for i in 0..len(input) {
        s = weigh(input[i])*s + input[i]
    }
    return s
}
"""

UNRECOGNIZED_SHAPE_HY = """
fn broken(input []uint8, s0 uint8) uint8 {
    s := s0
    for i in 0..len(input) {
        if input[i] == 0 {
            break
        }
        s = (3+input[i])*s + input[i]
    }
    return s
}
"""


def verify_p1_extraction() -> dict:
    admitted = extract_affine_loop(ADMITTED_HY)
    state_dependent = extract_affine_loop(STATE_DEPENDENT_HY)
    effectful = extract_affine_loop(EFFECTFUL_CALL_HY)
    unrecognized = extract_affine_loop(UNRECOGNIZED_SHAPE_HY)
    return {
        "admits_narrow_form": admitted.admitted and admitted.spec is not None and admitted.spec.width_bits == 8,
        "refuses_state_dependent": not state_dependent.admitted and "state" in state_dependent.reason,
        "refuses_effectful_call": not effectful.admitted and "function" in effectful.reason,
        "refuses_unrecognized_shape": not unrecognized.admitted,
    }


def verify_p4_quadratic_admission_hazard() -> dict:
    witnesses = [quadratic_admission_hazard(m) for m in (256, 65536, 2 ** 32, 2 ** 64, 1009)]
    fired = all(predicted != actual for predicted, actual in witnesses)
    return {"witnesses": witnesses, "fired": fired}


def check_all(live_go: bool = False) -> dict:
    p1 = verify_p1_extraction()
    p1_ok = all(p1.values())

    randomized = run_randomized_verification()
    p2_ok = randomized["ordered_all_match"]
    p3_fired = (randomized["multi_chunk_trials"] > 0
                and randomized["multi_chunk_mutant_mismatches"] > randomized["multi_chunk_trials"] // 2)

    p4 = verify_p4_quadratic_admission_hazard()
    p4_fired = p4["fired"]

    live = {"skipped": True, "reason": "live-go not requested"}
    p5_ok = True
    if live_go:
        admitted = extract_affine_loop(ADMITTED_HY)
        assert admitted.spec is not None
        live = run_live_go_verification(admitted.spec)
        if not live.get("skipped"):
            p5_ok = bool(live.get("build_ok") and live.get("run_ok") and live.get("ordered_all_match") and live.get("mutant_mismatches", 0) > 0)

    verified = p1_ok and p2_ok and p3_fired and p4_fired and p5_ok
    return {
        "verified": verified,
        "P1_extraction": p1,
        "P1_ok": p1_ok,
        "P2_randomized": randomized,
        "P2_ok": p2_ok,
        "P3_mutant_fired": p3_fired,
        "P4_quadratic_hazard": p4,
        "P4_fired": p4_fired,
        "P5_live_go": live,
        "P5_ok": p5_ok,
    }


def main(argv=None):
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--check", action="store_true")
    parser.add_argument("--json", action="store_true")
    parser.add_argument("--live-go", action="store_true")
    args = parser.parse_args(argv)

    result = check_all(live_go=args.live_go)
    if args.json:
        print(json.dumps(result, indent=2, sort_keys=True))
    else:
        print(f"W-4851 ordered-chunk-transformers: verified={result['verified']} "
              f"P1={result['P1_ok']} P2={result['P2_ok']} P3_fired={result['P3_mutant_fired']} "
              f"P4_fired={result['P4_fired']} P5={result['P5_ok']}")
    return 0 if (result["verified"] or not args.check) else 1


if __name__ == "__main__":
    sys.exit(main())
