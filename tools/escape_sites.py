#!/usr/bin/env python3
"""escape_sites.py — Find capture-write-at-foreign-call-boundary triples.

M1 deliverable for W-016 wildcard (A5 Go-escape check-site).

Scans a .hy file and emits every (capture, closure, foreign-call) triple
where:
  1. A closure captures a mutable binding from its enclosing scope
  2. The closure body writes (assigns to) that captured binding
  3. The closure is passed as an argument to a foreign (Go package) call

Scanning rules (post-fix, 2026-10-31):
  - Comments and string-literal contents are masked out in a single
    position-preserving pass BEFORE any structural scanning, so a `//`
    inside a string (URLs) or a `{`/`}` inside an interpolation string
    can no longer unbalance the brace/paren walks.
  - Closures with optional return types (`fn() int { ... }`,
    `fn() (int, error) { ... }`) are recognized while bare `fn(...)` type
    annotations without a body block are skipped.
  - Closures nested inside other closures are scanned (the scan resumes
    inside each body after processing it); every `fn` is processed once,
    outermost first, with nested closure bodies masked while extracting
    writes for the enclosing closure.
  - Closure parameters and closure-local declarations (`:=`, `var`, `let`,
    `const`) are tracked per closure and excluded from outer capture-writes.
  - Capture-writes cover plain, multi-LHS, compound (`+=`, `-=`, `*=`, `/=`,
    `%=`, `&=`, `|=`, `^=`, `<<=`, `>>=`), and inc/dec (`++`, `--`) mutations,
    attributing field/index place writes (`state.hits++`, `counts["k"] += 1`)
    to the root binding (`state`, `counts`).
  - The foreign-call boundary is the OUTERMOST qualified (pkg.fn) call on
    the enclosing-paren chain: in
    `mux.handle("/ping", http.handler(fn(...) {...}))` the closure
    escapes through `mux.handle`, so that is the reported call.

Usage:
    python tools/escape_sites.py <file.hy>
    python tools/escape_sites.py --self-test

M1 kill criteria:
  - examples/06-http-server/program.hy must yield exactly 0 triples
  - tests/checker-corpus/10-a5-http-handler-capture.hy must yield exactly 1 triple
"""

import json
from pathlib import Path
import re
import sys

KEYWORDS = {
    "fn", "var", "let", "const", "type", "struct", "interface", "return",
    "if", "else", "for", "range", "match", "select", "case", "default",
    "break", "continue", "defer", "task", "move", "true", "false", "nil",
    "none", "some", "ok", "err", "_",
}


def mask_comments_and_strings(src: str) -> str:
    """Blank comment text and string-literal contents, preserving length.

    Every character position is kept: newlines and quote delimiters stay,
    everything they hide becomes a space. Downstream structural scanning
    (brace/paren matching, write-regexes, line numbering) therefore sees
    a string-safe view of the source without any position drift.
    """
    out = list(src)
    n = len(src)
    i = 0
    while i < n:
        c = src[i]
        if c == '/' and i + 1 < n and src[i + 1] == '/':
            # Line comment: blank up to (not including) the newline.
            while i < n and src[i] != '\n':
                out[i] = ' '
                i += 1
        elif c == '/' and i + 1 < n and src[i + 1] == '*':
            # Block comment: blank through the closing `*/`.
            out[i] = ' '
            out[i + 1] = ' '
            i += 2
            while i < n:
                if src[i] == '*' and i + 1 < n and src[i + 1] == '/':
                    out[i] = ' '
                    out[i + 1] = ' '
                    i += 2
                    break
                if src[i] != '\n':
                    out[i] = ' '
                i += 1
        elif c == '"' or c == '`' or c == "'":
            # String literal: keep the delimiters, blank the contents.
            # Backslash escapes are consumed pairwise (raw strings skip them).
            quote = c
            i += 1
            while i < n:
                if quote != '`' and src[i] == '\\' and i + 1 < n:
                    out[i] = ' '
                    out[i + 1] = ' '
                    i += 2
                    continue
                if src[i] == quote:
                    i += 1
                    break
                if src[i] != '\n':
                    out[i] = ' '
                i += 1
        else:
            i += 1
    return ''.join(out)


def _qualified_callee_before_paren(clean_src: str, open_idx: int):
    """Return the dotted callee name immediately before clean_src[open_idx]=='('.

    Returns e.g. 'mux.handle' for `mux.handle(`; None when the token before
    the paren is not a dotted name (local fns, bare builtins, expressions).
    """
    k = open_idx - 1
    while k >= 0 and clean_src[k] in ' \t\n':
        k -= 1
    end = k
    while k >= 0 and (clean_src[k].isalnum() or clean_src[k] in '._'):
        k -= 1
    callee = clean_src[k + 1:end + 1].strip()
    if re.fullmatch(r'[a-zA-Z_]\w*(\.[a-zA-Z_]\w*)+', callee):
        return callee
    return None


def _find_closure_body_open(clean_src: str, after_params_idx: int) -> int:
    """Locate the opening '{' of a closure body after its parameter list `(...)`.

    Accepts optional return type annotations (`int`, `(int, error)`, `[]byte`,
    `map[string]int`, `*pkg.Type`, etc.) between `)` and `{`. Returns -1 when
    the `fn(...)` is a type signature without a body block.
    """
    n = len(clean_src)
    k = after_params_idx
    paren_depth = 0
    bracket_depth = 0
    while k < n:
        c = clean_src[k]
        if paren_depth == 0 and bracket_depth == 0:
            if c == '{':
                return k
            if c in ' \t\r\n':
                k += 1
                continue
            if c == '(':
                paren_depth += 1
                k += 1
                continue
            if c == '[':
                bracket_depth += 1
                k += 1
                continue
            if c.isalnum() or c in '_.*,':
                k += 1
                continue
            return -1
        else:
            if c == '(':
                paren_depth += 1
            elif c == ')':
                paren_depth -= 1
            elif c == '[':
                bracket_depth += 1
            elif c == ']':
                bracket_depth -= 1
            elif c == '{':
                return -1
            k += 1
    return -1


def _extract_param_names(params_src: str) -> set:
    """Extract parameter binding names from inside `fn(...)`."""
    chars = list(params_src)
    paren_depth = 0
    bracket_depth = 0
    for idx, c in enumerate(chars):
        if c == '(':
            paren_depth += 1
            chars[idx] = ' '
        elif c == ')':
            paren_depth = max(0, paren_depth - 1)
            chars[idx] = ' '
        elif c == '[':
            bracket_depth += 1
            chars[idx] = ' '
        elif c == ']':
            bracket_depth = max(0, bracket_depth - 1)
            chars[idx] = ' '
        elif paren_depth > 0 or bracket_depth > 0:
            chars[idx] = ' '
    flat = ''.join(chars)
    names = set()
    for part in flat.split(','):
        tokens = re.findall(r'[a-zA-Z_]\w*', part)
        if tokens and tokens[0] not in KEYWORDS:
            names.add(tokens[0])
    return names


def _mask_nested_closures(body_src: str) -> str:
    """Blank nested `fn(...) ... { ... }` literals inside `body_src`."""
    out = list(body_src)
    n = len(body_src)
    i = 0
    while i < n:
        fn_idx = body_src.find('fn', i)
        if fn_idx == -1:
            break
        if fn_idx > 0 and (body_src[fn_idx - 1].isalnum() or body_src[fn_idx - 1] == '_'):
            i = fn_idx + 2
            continue
        if fn_idx + 2 >= n or body_src[fn_idx + 2] != '(':
            i = fn_idx + 2
            continue
        depth = 1
        j = fn_idx + 3
        while j < n and depth > 0:
            if body_src[j] == '(':
                depth += 1
            elif body_src[j] == ')':
                depth -= 1
            j += 1
        if depth != 0:
            i = fn_idx + 2
            continue
        b_start = _find_closure_body_open(body_src, j)
        if b_start == -1:
            i = j
            continue
        b_depth = 1
        b_end = b_start + 1
        while b_end < n and b_depth > 0:
            if body_src[b_end] == '{':
                b_depth += 1
            elif body_src[b_end] == '}':
                b_depth -= 1
            b_end += 1
        if b_depth != 0:
            i = j
            continue
        for k in range(fn_idx, b_end):
            if out[k] != '\n':
                out[k] = ' '
        i = b_end
    return ''.join(out)


def _extract_closure_writes(params_src: str, body_src: str) -> set:
    """Return outer variables mutated inside `body_src` (excluding locals/params)."""
    own_body = _mask_nested_closures(body_src)
    locals_set = _extract_param_names(params_src)

    # Short declarations: `x := ...`, `a, b := ...`, `for i, v := range ...`
    for m in re.finditer(r'([a-zA-Z_]\w*(?:\s*,\s*[a-zA-Z_]\w*)*)\s*:=', own_body):
        for ident in re.findall(r'[a-zA-Z_]\w*', m.group(1)):
            if ident not in KEYWORDS:
                locals_set.add(ident)

    # `var` / `let` / `const` declarations: record declared names and blank the
    # declaration LHS through `=` so `var x int = 1` does not trigger a write on `int`.
    body_chars = list(own_body)
    decl_pat = re.compile(
        r'\b(?:var|let|const)\s+([a-zA-Z_]\w*(?:\s*,\s*[a-zA-Z_]\w*)*)([^\n;=]*=\s*(?!=))?'
    )
    for m in decl_pat.finditer(own_body):
        for ident in re.findall(r'[a-zA-Z_]\w*', m.group(1)):
            if ident not in KEYWORDS:
                locals_set.add(ident)
        for k in range(m.start(), m.end()):
            if body_chars[k] != '\n':
                body_chars[k] = ' '
    scan_body = ''.join(body_chars)

    place_pat = r'(?<![.\w])([a-zA-Z_]\w*)(?:(?:\.[a-zA-Z_]\w*)|(?:\[[^\]]*\]))*'
    writes = set()

    # Increment / decrement: `x++`, `x--`, `state.hits++`, `counts["k"]--`
    for m in re.finditer(place_pat + r'\s*(?:\+\+|--)', scan_body):
        writes.add(m.group(1))

    # Compound assignments: `+=`, `-=`, `*=`, `/=`, `%=`, `&=`, `|=`, `^=`, `<<=`, `>>=`
    for m in re.finditer(
        place_pat + r'\s*(?:\+=|-=|\*=|/=|%=|&=|\|=|\^=|<<=|>>=)(?!=)',
        scan_body,
    ):
        writes.add(m.group(1))

    # Plain assignments (single or multi-LHS): `x = 1`, `state.hits = 1`, `a, b = 1, 2`
    lhs_item = r'(?<![.\w])[a-zA-Z_]\w*(?:(?:\.[a-zA-Z_]\w*)|(?:\[[^\]]*\]))*'
    multi_assign_pat = re.compile(
        r'(' + lhs_item + r'(?:\s*,\s*' + lhs_item + r')*)\s*(?<![:!<>+\-*/%&|^=])=\s*(?!=)'
    )
    for m in multi_assign_pat.finditer(scan_body):
        lhs_list = m.group(1)
        for item in lhs_list.split(','):
            root_match = re.match(r'\s*([a-zA-Z_]\w*)', item)
            if root_match:
                writes.add(root_match.group(1))

    return {w for w in writes if w not in locals_set and w not in KEYWORDS}


def find_escape_sites(src: str) -> list:
    """Find capture-write-at-foreign-call triples in .hy source.

    Returns list of dicts: {'capture': str, 'closure_line': int, 'foreign_call': str}
    """
    clean_src = mask_comments_and_strings(src)

    triples = []

    i = 0
    while i < len(clean_src):
        fn_idx = clean_src.find('fn', i)
        if fn_idx == -1:
            break
        if fn_idx > 0 and (clean_src[fn_idx - 1].isalnum() or clean_src[fn_idx - 1] == '_'):
            i = fn_idx + 2
            continue
        if fn_idx + 2 >= len(clean_src) or clean_src[fn_idx + 2] != '(':
            i = fn_idx + 2
            continue

        # Match parens from after 'fn('
        depth = 1
        j = fn_idx + 3
        while j < len(clean_src) and depth > 0:
            c = clean_src[j]
            if c == '(':
                depth += 1
            elif c == ')':
                depth -= 1
            j += 1
        if depth != 0:
            i = fn_idx + 2
            continue

        body_start = _find_closure_body_open(clean_src, j)
        if body_start == -1:
            i = j
            continue

        # Match braces for the body (strings are masked, so this is safe)
        body_depth = 1
        body_end = body_start + 1
        while body_end < len(clean_src) and body_depth > 0:
            c = clean_src[body_end]
            if c == '{':
                body_depth += 1
            elif c == '}':
                body_depth -= 1
            body_end += 1
        if body_depth != 0:
            i = j
            continue

        params_src = clean_src[fn_idx + 3:j - 1]
        body = clean_src[body_start + 1:body_end - 1]
        captures = _extract_closure_writes(params_src, body)

        # Walk the enclosing-paren chain outward; the OUTERMOST qualified
        # (pkg.fn) callee on the chain is the reported boundary.
        foreign_call = None
        pos = fn_idx
        while True:
            before = clean_src[:pos]
            paren_depth = 0
            last_open = -1
            for k in range(len(before) - 1, -1, -1):
                c = before[k]
                if c == ')':
                    paren_depth += 1
                elif c == '(':
                    if paren_depth == 0:
                        last_open = k
                        break
                    paren_depth -= 1
            if last_open == -1:
                break
            callee = _qualified_callee_before_paren(clean_src, last_open)
            if callee:
                foreign_call = callee
            pos = last_open

        if captures and foreign_call:
            line_no = clean_src[:fn_idx].count('\n') + 1
            for cap in sorted(captures):
                triples.append({
                    'capture': cap,
                    'closure_line': line_no,
                    'foreign_call': foreign_call,
                })

        # Resume INSIDE the body so closures nested in this one are scanned too.
        i = body_start + 1

    return triples


def run_self_test() -> int:
    repo_root = Path(__file__).resolve().parent.parent
    cases_path = Path(__file__).resolve().parent / "tests" / "escape_sites_cases.json"
    spec = json.loads(cases_path.read_text(encoding="utf-8"))
    for case in spec["cases"]:
        if "file" in case:
            src = (repo_root / case["file"]).read_text(encoding="utf-8")
        else:
            src = case["source"]
        triples = find_escape_sites(src)
        if len(triples) != case["expected_count"]:
            print(
                f"FAIL {case['name']}: expected {case['expected_count']} triples, got {triples}",
                file=sys.stderr,
            )
            return 1
        if "expected_triples" in case and triples != case["expected_triples"]:
            print(
                f"FAIL {case['name']}: expected {case['expected_triples']}, got {triples}",
                file=sys.stderr,
            )
            return 1
    print(f"OK: {len(spec['cases'])} escape_sites cases passed")
    return 0


def main():
    if len(sys.argv) < 2:
        print("Usage: python escape_sites.py <file.hy>", file=sys.stderr)
        sys.exit(1)

    if sys.argv[1] == "--self-test":
        sys.exit(run_self_test())

    filepath = sys.argv[1]
    with open(filepath, 'r', encoding='utf-8') as f:
        src = f.read()

    triples = find_escape_sites(src)

    for t in triples:
        print(f"capture={t['capture']} closure_line={t['closure_line']} foreign_call={t['foreign_call']}")

    print(f"Total escape-with-write triples: {len(triples)}")

    if triples:
        sys.exit(0)
    else:
        sys.exit(1)


if __name__ == '__main__':
    main()
