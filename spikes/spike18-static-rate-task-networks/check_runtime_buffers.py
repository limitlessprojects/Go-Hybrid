"""Independent runtime buffer check for the W-018 packaged spike.

Usage: python3 check_runtime_buffers.py [SPIKE_DIRECTORY] [GO_EXECUTABLE]
Copies the Go fixture into a temporary directory. No repository files change.
Checks four kernels for 10,000 periods, then requires a broken pop to fail.
"""
import json
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile


def run():
    spike_dir = Path(sys.argv[1]).resolve() if len(sys.argv) > 1 else Path(__file__).parent.resolve()
    go_arg = sys.argv[2] if len(sys.argv) > 2 else "go"
    go_cmd = shutil.which(go_arg) or go_arg

    certificates = json.loads((spike_dir / 'certificates.json').read_text(encoding='utf-8'))
    code = (spike_dir / 'generated.go').read_text(encoding='utf-8')
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

    with tempfile.TemporaryDirectory(prefix='w018-guards-') as tmp:
        root = Path(tmp)
        (root / 'go.mod').write_bytes((spike_dir / 'go.mod').read_bytes())
        (root / 'generated.go').write_text(code, encoding='utf-8')
        (root / 'runtime_buffers_test.go').write_text('\n'.join(tests), encoding='utf-8')
        try:
            positive = subprocess.run([go_cmd, 'test', '-count=1', '-v', './...'], cwd=root,
                                      text=True, capture_output=True, timeout=120)
        except FileNotFoundError:
            print(f"SKIPPED: Go executable '{go_cmd}' not found. Test generation verified.")
            return

        print(positive.stdout, end='')
        if positive.returncode:
            raise RuntimeError(positive.stderr + 'guarded kernels failed')
        # Removing the first pop's decrement makes the next period overflow.
        mutated = code.replace('k.n0--;', '/* deliberate missing pop decrement */', 1)
        assert mutated != code
        (root / 'generated.go').write_text(mutated, encoding='utf-8')
        (root / 'negative_control_test.go').write_text(
            'package staticnetwork\nimport "testing"\n'
            'func TestBrokenPop(t *testing.T) { k := NewKernel0(); k.Step(); k.Step() }\n', encoding='utf-8')
        negative = subprocess.run([go_cmd, 'test', '-count=1', '-run', '^TestBrokenPop$', './...'],
                                  cwd=root, text=True, capture_output=True, timeout=120)
        if negative.returncode == 0 or 'buffer overflow' not in negative.stdout + negative.stderr:
            raise RuntimeError('negative control was not detected: ' + negative.stdout + negative.stderr)
        print('NEGATIVE CONTROL: broken pop rejected with buffer overflow (expected).')
        print('PASS: 9 pop + 9 push guards; 4 kernels x 10,000 periods; marking checked every period.')


if __name__ == '__main__':
    run()
