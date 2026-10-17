"""Small SDF witness producer, independent checker and restricted Go emitter.

Only fixed-rate uint64 source/sum/scale/sink actors are admitted. No foreign
functions, callbacks, maps, pointers or dynamic topology. uint64 wraps modulo 2^64.
"""
from dataclasses import dataclass
from fractions import Fraction
from functools import reduce
from math import gcd, lcm

MASK = (1 << 64) - 1


@dataclass(frozen=True)
class Actor:
    name: str
    op: str
    factor: int = 1


@dataclass(frozen=True)
class Edge:
    source: str
    target: str
    produce: int = 1
    consume: int = 1
    initial: tuple = ()


def validate(actors, edges):
    names = [a.name for a in actors]
    if not names or len(set(names)) != len(names):
        raise ValueError('empty or duplicate actors')
    for e in edges:
        if e.source not in names or e.target not in names:
            raise ValueError('unknown endpoint')
        if type(e.produce) is not int or type(e.consume) is not int or min(e.produce, e.consume) <= 0:
            raise ValueError('rates must be positive integers')
        if e.produce > 4096 or e.consume > 4096 or len(e.initial) > 4096:
            raise ValueError('spike size limit')
        if any(type(v) is not int or not 0 <= v <= MASK for v in e.initial):
            raise ValueError('tokens must be uint64')
    for a in actors:
        ins = [e for e in edges if e.target == a.name]
        outs = [e for e in edges if e.source == a.name]
        if a.op not in ('source', 'sum', 'scale', 'sink'):
            raise ValueError('unmodeled actor effect')
        if type(a.factor) is not int or not 0 <= a.factor <= MASK:
            raise ValueError('factor must be uint64')
        if a.op == 'source' and (ins or not outs):
            raise ValueError('source shape')
        if a.op == 'sink' and (not ins or outs):
            raise ValueError('sink shape')
        if a.op in ('sum', 'scale') and (not ins or not outs):
            raise ValueError('transform shape')


def repetition(actors, edges):
    validate(actors, edges)
    result = {}
    for root in actors:
        if root.name in result:
            continue
        component = {root.name: Fraction(1)}
        pending = [root.name]
        while pending:
            n = pending.pop()
            for e in edges:
                if e.source == n:
                    peer, ratio = e.target, Fraction(e.produce, e.consume)
                elif e.target == n:
                    peer, ratio = e.source, Fraction(e.consume, e.produce)
                else:
                    continue
                value = component[n] * ratio
                if peer in component:
                    if component[peer] != value:
                        raise ValueError('inconsistent balance equations')
                else:
                    component[peer] = value
                    pending.append(peer)
        denom = lcm(*(v.denominator for v in component.values()))
        values = {k: int(v * denom) for k, v in component.items()}
        divisor = reduce(gcd, values.values())
        result.update({k: v // divisor for k, v in values.items()})
    if sum(result.values()) > 4096:
        raise ValueError('period exceeds spike limit')
    return result


def plan(actors, edges):
    q = repetition(actors, edges)
    remaining = q.copy()
    marking = [len(e.initial) for e in edges]
    peaks = marking.copy()
    schedule = []
    while any(remaining.values()):
        for a in actors:
            if remaining[a.name] and all(marking[i] >= e.consume for i, e in enumerate(edges) if e.target == a.name):
                break
        else:
            raise ValueError('no enabled actor: no schedule witness')
        for i, e in enumerate(edges):
            if e.target == a.name:
                marking[i] -= e.consume
        for i, e in enumerate(edges):
            if e.source == a.name:
                marking[i] += e.produce
                peaks[i] = max(peaks[i], marking[i])
        schedule.append(a.name)
        remaining[a.name] -= 1
    certificate = {'schedule': schedule, 'capacities': peaks}
    verify(actors, edges, certificate)
    return certificate


def verify(actors, edges, certificate, budget_bytes=None):
    """Do not trust the planner: independently replay its claimed schedule.

    This checks a witness, not optimality or completeness of schedule search.
    Buffer budget is token payload bytes only, not process memory or structs.
    """
    validate(actors, edges)
    schedule = certificate['schedule']
    caps = certificate['capacities']
    if not schedule or len(schedule) > 4096 or len(caps) != len(edges):
        raise ValueError('invalid certificate dimensions')
    if any(type(c) is not int or c <= 0 or c > 65536 for c in caps):
        raise ValueError('invalid capacity')
    counts = {a.name: 0 for a in actors}
    marking = [len(e.initial) for e in edges]
    peaks = marking.copy()
    if any(m > c for m, c in zip(marking, caps)):
        raise ValueError('initial overflow')
    for name in schedule:
        if name not in counts:
            raise ValueError('unknown scheduled actor')
        for i, e in enumerate(edges):
            if e.target == name:
                if marking[i] < e.consume:
                    raise ValueError('token underflow')
                marking[i] -= e.consume
        for i, e in enumerate(edges):
            if e.source == name:
                marking[i] += e.produce
                if marking[i] > caps[i]:
                    raise ValueError('token overflow')
                peaks[i] = max(peaks[i], marking[i])
        counts[name] += 1
    if not all(counts.values()) or marking != [len(e.initial) for e in edges]:
        raise ValueError('not a complete periodic schedule')
    if peaks != caps:
        raise ValueError('capacity does not match witnessed high-water mark')
    if budget_bytes is not None and 8 * sum(caps) > budget_bytes:
        raise ValueError('token payload budget exceeded')
    return {'firings': counts, 'token_payload_bytes': 8 * sum(caps)}


def reference(actors, edges, certificate, periods):
    from collections import deque
    verify(actors, edges, certificate)
    queues = [deque(e.initial) for e in edges]
    table = {a.name: a for a in actors}
    next_token = checksum = 0
    for _ in range(periods):
        for name in certificate['schedule']:
            a = table[name]
            value = 0
            for i, e in enumerate(edges):
                if e.target == name:
                    for _ in range(e.consume):
                        value = (value + queues[i].popleft()) & MASK
            if a.op == 'source':
                next_token = (next_token + 1) & MASK
                value = next_token
            elif a.op == 'scale':
                value = value * a.factor & MASK
            elif a.op == 'sink':
                checksum = (checksum + value) & MASK
            for i, e in enumerate(edges):
                if e.source == name:
                    queues[i].extend([value] * e.produce)
    return next_token, checksum, [list(q) for q in queues]


def emit_go(actors, edges, certificate, name):
    """Emit only from validated enums/integers; actor labels never become Go code."""
    verify(actors, edges, certificate)
    if not name.isascii() or not name.isidentifier():
        raise ValueError('invalid Go type name')
    caps = certificate['capacities']
    out = [f'type {name} struct {{ Next, Checksum uint64']
    for i, c in enumerate(caps):
        out.append(f'q{i} [{c}]uint64; h{i}, n{i} int')
    out.append('}')
    out.append(f'func New{name}() {name} {{ var k {name}')
    for i, e in enumerate(edges):
        for j, v in enumerate(e.initial):
            out.append(f'k.q{i}[{j}] = {v}')
        out.append(f'k.n{i} = {len(e.initial)}')
    out.append('return k\n}')
    for i, c in enumerate(caps):
        out.append(f'func (k *{name}) pop{i}() uint64 {{ v := k.q{i}[k.h{i}]; k.h{i} = (k.h{i}+1)%{c}; k.n{i}--; return v }}')
        out.append(f'func (k *{name}) push{i}(v uint64) {{ k.q{i}[(k.h{i}+k.n{i})%{c}] = v; k.n{i}++ }}')
    out.append(f'func (k *{name}) Step() {{')
    table = {a.name: a for a in actors}
    for label in certificate['schedule']:
        a = table[label]
        out.append('{ var v uint64')
        for i, e in enumerate(edges):
            if e.target == label:
                for _ in range(e.consume):
                    out.append(f'v += k.pop{i}()')
        if a.op == 'source':
            out.append('k.Next++; v = k.Next')
        elif a.op == 'scale':
            out.append(f'v *= {a.factor}')
        elif a.op == 'sink':
            out.append('k.Checksum += v')
        for i, e in enumerate(edges):
            if e.source == label:
                for _ in range(e.produce):
                    out.append(f'k.push{i}(v)')
        out.append('}')
    out.append('}')
    return '\n'.join(out)
