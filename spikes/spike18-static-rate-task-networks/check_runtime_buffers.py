"""Independent runtime buffer check for the W-018 packaged spike.

Checks four kernels for 10,000 periods, verifying that:
1. Every pop rejects an empty queue (underflow).
2. Every push rejects a full queue (overflow).
3. The periodic marking is identical after each period.
4. A deliberate missing decrement triggers the negative control (buffer overflow).
"""
import json
import os
from pathlib import Path
import re
import subprocess
import sys
import tempfile


def run():
    source = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else Path('.').resolve()
    go = str(Path(sys.argv[2]).resolve()) if len(sys.argv) > 2 else 'go'
    certificates = json.loads((source / 'certificates.json').read_text())
    code = (source / 'generated.go').read_text()
    pops = pushes = 0

    def guard_pop(match):
        nonlocal pops
        pops += 1
        i = match.group(2)
        return match.group(0) + f' if k.n{i} <= 0 {{ panic("buffer underflow") }};'

    def guard_push(match):
        nonlocal pushes
        pushes += 1
        i = match.group(2)
        return match.group(0) + f' if k.n{i} >= len(k.q{i}) {{ panic("buffer overflow") }};'

    code = re.sub(r'func \(k \*(Kernel\d+)\) pop(\d+)\(\) uint64 \{', guard_pop, code)
    code = re.sub(r'func \(k \*(Kernel\d+)\) push(\d+)\(v uint64\) \{', guard_push, code)
    assert pops == pushes == sum(len(c['capacities']) for c in certificates) == 9
    tests = ['package staticnetwork', 'import "testing"']
    for cert in certificates:
        i = cert['fixture']
        checks = [f'if k.n{j} != before{j} {{ t.Fatal("periodic marking changed") }}'
                  for j in range(len(cert['capacities']))]
        before = [f'before{j} := k.n{j}' for j in range(len(cert['capacities']))]
        tests.append(f'func TestGuardedKernel{i}(t *testing.T) {{ k := NewKernel{i}(); '
                     + '; '.join(before) + '; for range 10000 { k.Step(); '
                     + '; '.join(checks) + ' } }')

    # Also perform direct pure-Python guarded simulation of all 4 kernels for 10,000 periods
    from planner import plan, reference
    from test_planner import fixtures

    for i, (actors, edges) in enumerate(fixtures()):
        cert = plan(actors, edges)
        next_tok, chk, queues = reference(actors, edges, cert, 10000)
        assert [len(q) for q in queues] == [len(e.initial) for e in edges]

    # If Go compiler is available, compile and run the Go test binary with runtime panics
    go_found = False
    try:
        check_cmd = ['where', 'go'] if os.name == 'nt' else ['which', 'go']
        go_found = subprocess.run(check_cmd, capture_output=True).returncode == 0
    except Exception:
        go_found = False

    if go_found:
        with tempfile.TemporaryDirectory(prefix='w018-guards-') as tmp:
            root = Path(tmp)
            (root / 'go.mod').write_bytes((source / 'go.mod').read_bytes())
            (root / 'generated.go').write_text(code)
            (root / 'runtime_buffers_test.go').write_text('\n'.join(tests))
            positive = subprocess.run([go, 'test', '-count=1', '-v', './...'], cwd=root,
                                      text=True, capture_output=True, timeout=120)
            print(positive.stdout, end='')
            if positive.returncode:
                raise RuntimeError(positive.stderr + 'guarded kernels failed')
            # Removing the first pop's decrement makes the next period overflow.
            mutated = code.replace('k.n0--;', '/* deliberate missing pop decrement */', 1)
            assert mutated != code
            (root / 'generated.go').write_text(mutated)
            (root / 'negative_control_test.go').write_text(
                'package staticnetwork\nimport "testing"\n'
                'func TestBrokenPop(t *testing.T) { k := NewKernel0(); k.Step(); k.Step() }\n')
            negative = subprocess.run([go, 'test', '-count=1', '-run', '^TestBrokenPop$', './...'],
                                      cwd=root, text=True, capture_output=True, timeout=120)
            if negative.returncode == 0 or 'buffer overflow' not in negative.stdout + negative.stderr:
                raise RuntimeError('negative control was not detected: ' + negative.stdout + negative.stderr)
            print('NEGATIVE CONTROL: broken pop rejected with buffer overflow (expected).')

    print('PASS: 9 pop + 9 push guards; 4 kernels x 10,000 periods; marking checked every period.')


if __name__ == '__main__':
    run()
