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
  - Closures nested inside other closures are scanned (the scan resumes
    inside each body after processing it); every `fn` is processed once,
    outermost first.
  - `x :=` inside a closure body is a LOCAL DECLARATION, not a write to
    an enclosing binding — it is not a capture. Capture-writes are the
    forms that necessarily reference an existing binding: `x =`, `x +=`,
    `x -=`, `x++`, `x--`.
  - The foreign-call boundary is the OUTERMOST qualified (pkg.fn) call on
    the enclosing-paren chain: in
    `mux.handle("/ping", http.handler(fn(...) {...}))` the closure
    escapes through `mux.handle`, so that is the reported call.

Known M1 limitation (documented, not hidden): the capture set of an
outer closure includes writes performed by closures nested inside it
when the outer closure is itself foreign-enclosed. The two kill-criteria
files below do not exercise that shape; M3 (the checker) must tighten it.

Usage:
    python tools/escape_sites.py <file.hy>

M1 kill criteria:
  - examples/06-http-server/program.hy must yield exactly 0 triples
  - tests/checker-corpus/10-a5-http-handler-capture.hy must yield exactly 1 triple
"""

import re
import sys


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


def find_escape_sites(src: str) -> list:
    """Find capture-write-at-foreign-call triples in .hy source.

    We use a lightweight recursive-descent scan over brace/paren structure
    to find fn(...) closures that:
      (a) contain a write to a variable defined outside the closure
          (`x =`, `x +=`, `x -=`, `x++`, `x--` — `x :=` declares a local
          and is deliberately NOT a capture), and
      (b) are passed as an argument to a call on a qualified name (pkg.fn)
          — the outermost qualified call on the enclosing chain is reported.

    Returns list of dicts: {'capture': str, 'closure_line': int, 'foreign_call': str}
    """
    clean_src = mask_comments_and_strings(src)

    triples = []

    # Find all `fn(` closures — look for `fn(` followed by paren-matching
    i = 0
    while i < len(clean_src):
        # Find next 'fn'
        fn_idx = clean_src.find('fn', i)
        if fn_idx == -1:
            break
        # Check it's a keyword, not part of another word
        if fn_idx > 0 and (clean_src[fn_idx - 1].isalnum() or clean_src[fn_idx - 1] == '_'):
            i = fn_idx + 2
            continue
        # Check next char is '('
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

        # Now we have fn(...) from fn_idx to j (exclusive of closing paren at j-1)
        # Find the body: after the parameter list parens, there should be '{'
        body_start = j
        while body_start < len(clean_src) and clean_src[body_start] in ' \t\n':
            body_start += 1
        if body_start >= len(clean_src) or clean_src[body_start] != '{':
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

        body = clean_src[body_start + 1:body_end - 1]

        # Find captured variables written in the body.
        # A capture-write necessarily references an EXISTING binding:
        #   x = ...   (plain assignment; == excluded)
        #   x += ...  / x -= ...
        #   x++       / x--
        # `x :=` is a short DEFINE — it declares a new local inside the
        # closure and is deliberately not treated as a capture-write.
        captures = set()

        for m in re.finditer(r'\b([a-zA-Z_]\w*)\s*\+=', body):
            captures.add(m.group(1))
        for m in re.finditer(r'\b([a-zA-Z_]\w*)\s*-=(?!=)', body):
            captures.add(m.group(1))
        for m in re.finditer(r'\b([a-zA-Z_]\w*)\s*\+\+', body):
            captures.add(m.group(1))
        for m in re.finditer(r'\b([a-zA-Z_]\w*)\s*--', body):
            captures.add(m.group(1))
        for m in re.finditer(r'\b([a-zA-Z_]\w*)\s*=\s*(?!=)', body):
            captures.add(m.group(1))

        # Check if this closure is passed to a foreign call.
        # Walk the enclosing-paren chain outward; the OUTERMOST qualified
        # (pkg.fn) callee on the chain is the reported boundary:
        #   mux.handle("/ping", http.handler(fn(...) {...}))
        #   -> chain calees: http.handler, mux.handle -> reports mux.handle.
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
                foreign_call = callee  # keep walking; outermost qualified wins
            pos = last_open

        if captures and foreign_call:
            line_no = clean_src[:fn_idx].count('\n') + 1
            for cap in sorted(captures):
                triples.append({
                    'capture': cap,
                    'closure_line': line_no,
                    'foreign_call': foreign_call,
                })

        # Resume INSIDE the body so closures nested in this one are scanned
        # too; each `fn` keyword is still processed exactly once because the
        # scan position always advances past the current fn's header.
        i = body_start + 1

    return triples


def main():
    if len(sys.argv) < 2:
        print("Usage: python escape_sites.py <file.hy>", file=sys.stderr)
        sys.exit(1)

    filepath = sys.argv[1]
    with open(filepath, 'r') as f:
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
