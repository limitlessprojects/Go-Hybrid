# CENSUS — does the corpus have typed narrow-width arithmetic? (reconnaissance)

**Label: exploratory reconnaissance, not a pre-registered hypothesis.** The two
pre-registered hypotheses live in `PREREGISTRATION.md`, and their predicted
table was committed before it was measured. This scan was run *before* it was
written up, so it is reported as reconnaissance, and what it produces is an
**amendment to the wildcard's M1 item 2** (below) — never a "prediction
confirmed" claim. Keeping that distinction is the point of the protocol.

Method: `corpus_numeric_census.py` (standard library; a lexical regex scan over
the `.hy` text, **no AST** — this is a sizing scan, not a soundness analysis).

```sh
python spikes/w3060-exact-value-arithmetic/corpus_numeric_census.py
```

## Numbers (on `main`, 130 corpus programs)

| measure | value |
|---|---|
| corpus programs | **130** |
| typed numeric declarations (`var/let/const <name> <kind>`) | **39** |
| … of which sub-word width (<64 bit, incl. `byte`/`rune`/`float32`) | 29 |
| files with any typed numeric declaration | **14 / 130** |
| files with ≥1 sub-word-width declaration | **8 / 130** |
| binary arithmetic operator occurrences (lexical) | **3174** |
| files containing ≥1 such operator | **130 / 130** |
| sites where **both** operand names carry a declared numeric type | **2** |
| … same width, sub-word | 2 |
| … width mismatch (which would be a Go build error) | 0 |

The only both-declared site, twice, in one file:
`typed-const-expression-controls-accept.hy: x(u8) + y(u8)`.

## What this establishes

1. **Arithmetic is not scarce; *typed* arithmetic is.** Every one of the 130
   programs contains arithmetic — 3174 operator occurrences — but nearly all of
   it runs on *inferred* (`:=`) types, and only 14 files declare a numeric type
   at all.
2. **The corpus cannot support M1 item 2 as written.** "The fraction of
   typed-numeric sites provably `exact`" would have a denominator of **2**, and
   a 50% defeat condition cannot be evaluated on a denominator of two.
3. **The narrow-width surface exists but is tiny** — 39 declarations, 29 of
   them sub-word, across 8 files. That is *consistent with* the value axis being
   structurally untested, and it is the same conclusion #3060 reached from the
   other direction (six `HYB-E6nnn` codes, all of them constant-only).

## The amendment (published before the M1 verdict)

M1 item 2 of the wildcard is restated:

- **as written:** static-discharge rate measured **over the 130 corpus programs**;
- **as amended:** the discharge rate is measured over a **generated narrow-width
  battery** — the generator becomes the M1 deliverable — and the corpus is used
  only as a **regression** check: all 130 verdicts must stay exactly as they are
  (`130/130`, already pinned by `tools/corpus_harness.py`).
- The **50% defeat condition keeps its strength**; it is now applied to the
  generator's sites, which is where a discharge rate is meaningful at all.
- **Consequence for the plan:** M1's first artifact is now the *generator*, not
  the interval pass. That ordering is strictly cheaper, because the generator is
  needed by the third falsifier (the reference-oracle census) anyway.

*Why publish this instead of quietly fixing the wording:* the wildcard's own
text committed to a corpus-based measurement, and the reconnaissance shows the
corpus cannot carry it. Recording that in the open — with the denominator that
killed it — is the protocol working, not a problem with the proposal.
