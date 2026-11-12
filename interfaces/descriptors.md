# Boundary descriptor ledger — per-callee stack classes

Row 4 of the post-MVP queue (gates/G2.md follow-up #2; Cut Log 2026-10-13).
The trampoline discipline: the fast path runs the Rust callee on the CALLER'S
goroutine stack after an in-trampoline headroom check. The guard proves
headroom **at entry and nothing after** — so a crossing may take a guarded
fast path ONLY if the callee's worst-case stack use is an UPPER BOUND below
that guard. This ledger is that argument, per fn. Reclassification is a
Decision Log event, never a silent edit.

## Classes

| class | guard | trampoline | meaning |
|-------|-------|------------|---------|
| SMALL | 1024 B | `Cross1S..4S` | callee's worst case ≤ 1 KiB, no recursion on input-proportional structures |
| DEFAULT | 4096 B | `Cross1..4` | bounded but not (yet) argued under 1 KiB |
| DEEP | — (1 MiB pool) | `CrossDeep*` | input-proportional or unbounded stack use |

## Curated surface (as of row 4)

### sha3binding — ALL SMALL

| fn | class | justification |
|----|-------|---------------|
| `FnAbiWord` | SMALL | constant return; zero recursion, zero heap, handful of bytes of frame |
| `FnDigest256` | SMALL | Keccak-f[1600] absorb/squeeze: fixed 200-byte state permutation in a loop; stack use is a constant independent of input length |
| `FnStreamChecksum` | SMALL | N permutations inside one call; the loop carries no stack growth (iters live in registers) |

### jsonbinding — SPLIT BY RECURSION

serde_json (pinned 1.0.140) recurses on value nesting:
`from_str` with `remaining_depth = 128` (src/de.rs), and `clone`/`drop`/
`to_writer` walk the tree. Worst-case stack ≈ 128 × frame — an order of
magnitude above the DEFAULT guard for parse. **Any fn that can touch a
nested value is DEEP regardless of the caller's input shape.**

| fn | class | justification |
|----|-------|---------------|
| `FnParse` | **DEEP** | `from_str` recursion, depth ≤ 128 — worst case ≫ 4096 B. (Pre-row-4 this crossed via the default 4096-guard fast path: **latent overflow on shallow goroutines for deep inputs** — the soundness note in jsonbinding/json.go; the class table + stress test are the fix) |
| `FnGetArray` | **DEEP** | `as_array().clone()` recurses on the array's contents |
| `FnArrayGet` | **DEEP** | element `.clone()` recurses on the element's nesting |
| `FnSet` | **DEEP** | `BTreeMap::insert` DROPS the replaced value; drop recurses |
| `FnRelease` | **DEEP** | `*slot = None` drops the value; drop recurses. Includes the cleanup-goroutine release (arbitrary stacks) |
| `FnToStringPretty` | **DEEP** | `to_writer` serializes by tree walk |
| `FnNewObject` / `FnNewArray` | **DEFAULT** | table insert is O(1) in the tree but `slots.push` can realloc; allocator slow path not argued under 1 KiB (reclassified from SMALL, issue #375) |
| `FnGetInt` | SMALL | BTreeMap lookup + scalar copy |
| `FnGetString` | **DEFAULT** | no tree walk, but `to_string()` + table insert reach malloc/realloc; allocator depth not argued under 1 KiB (reclassified from SMALL, issue #375) |
| `FnArrayLen` | SMALL | Vec len read |
| `FnPushString` | **DEFAULT** | `to_string()` + Vec push reach malloc/realloc; same allocator argument (reclassified from SMALL, issue #375) |

### regexbinding — ALL DEEP

Both `hy_regex_is_match` and `hy_regex_is_valid` compile the pattern via
`regex::Regex::new(pat)` on every call. Disassembly of `libhyrt.a` shows:
(1) `regex_automata::meta::strategy::new` unconditionally allocates a **7,648 B**
single frame (with an acyclic compilation call chain up to **22,976 B** through
`RegexBuilder::build` → `meta::regex::Builder::build` → `meta::strategy::new` →
`Choice::new` → `Teddy::new` → `aho_corasick::nfa::noncontiguous::Builder::build`
→ `driftsort_main`), exceeding the 4,096 B `DEFAULT` guard even for a 1-byte
pattern; and (2) dropping the parsed `regex_syntax::ast::Ast` / `ClassSet`
recurses on bracket nesting depth (`drop_in_place::<Ast>` ↔ `ClassSet::drop`).

| fn | class | justification |
|----|-------|---------------|
| `FnIsMatch` | **DEEP** | `Regex::new` per call: 7,648 B `meta::strategy::new` frame (22,976 B acyclic compile chain) + recursive `Ast`/`ClassSet` drop on nested character classes ≫ 4096 B |
| `FnIsValid` | **DEEP** | `Regex::new` per call: same 7,648 B `meta::strategy::new` frame (22,752 B acyclic compile chain) + recursive `Ast`/`ClassSet` drop ≫ 4096 B |

### hyrt internals

| fn | class | justification |
|----|-------|---------------|
| `hy_nop` (G2 bench) | DEFAULT | the G2 gate's measured subject — its recorded evidence (3.37/3.44 ns) is for THIS class; never reclassify (bench-overhead small-class scenarios use their own fn) |

## The regression that guards this ledger

- `TestDeepNestingOnConstrainedStack` (jsonbinding): 128-deep JSON through
  parse/get_array/array_get/set/to_string_pretty/release on a
  stack-constrained goroutine — the DEEP-class routes are the fix's proof.
- `TestSmallClassFastPathOnShallowStack` (hybridge): a controlled-headroom
  goroutine in the (1024, 4096) window takes the fast path through `Cross*S`
  while the default class takes the deep path — DeepPathCount is the oracle.
- The offsets canary (`SPHeadroomForTest` + extract_offsets.py) is unchanged:
  both guard classes read the same `g.stackguard0` offset.

## Adding a curated fn

New fns enter at DEFAULT with a one-line justification, get measured/argued,
and move to SMALL only through this ledger + a Decision Log entry. Anything
that CAN recurse on input-proportional structure (a serde value, a regex over
unbounded text with backtracking, a recursive descent) is DEEP by rule, not
by measurement.
