"""Small explicit CFG model for #1086; not a parser or a Hybrid checker.

Structural SSA validity and dynamic ownership are deliberately separate.
Phi operands are edge uses; all other operands are instruction-local uses.
"""

from dataclasses import dataclass


@dataclass(frozen=True)
class Instruction:
    op: str
    dest: str = ""
    args: tuple = ()
    incoming: tuple = ()  # (predecessor block, SSA value), only for phi


@dataclass(frozen=True)
class Block:
    instructions: tuple = ()
    successors: tuple = ()


@dataclass(frozen=True)
class Graph:
    blocks: dict
    entry: str = "entry"


class InvalidSSA(ValueError):
    pass


def verify(graph):
    """Check reachable CFG, unique definitions, dominance, and phi edges.

    This deliberately does not count consumes or infer alias/loan effects:
    those are ownership semantics, not ordinary SSA dominance constraints.
    """
    blocks = graph.blocks
    if graph.entry not in blocks:
        raise InvalidSSA("missing entry")
    predecessors = {name: set() for name in blocks}
    for name, block in blocks.items():
        if len(set(block.successors)) != len(block.successors):
            raise InvalidSSA("duplicate successor")
        for successor in block.successors:
            if successor not in blocks:
                raise InvalidSSA("unknown successor")
            predecessors[successor].add(name)
    if predecessors[graph.entry]:
        raise InvalidSSA("entry has predecessors")
    reached, pending = set(), [graph.entry]
    while pending:
        name = pending.pop()
        if name not in reached:
            reached.add(name)
            pending.extend(blocks[name].successors)
    if reached != set(blocks):
        raise InvalidSSA("unreachable block")

    dominators = {name: set(blocks) for name in blocks}
    dominators[graph.entry] = {graph.entry}
    changed = True
    while changed:
        changed = False
        for name in blocks:
            if name == graph.entry:
                continue
            new = {name} | set.intersection(
                *(dominators[pred] for pred in predecessors[name]))
            if new != dominators[name]:
                dominators[name] = new
                changed = True

    # Shape validation is independent of the operand's ownership state.
    arities = {"alloc": 0, "alias": 1, "clone": 1,
               "consume": 1, "write": 1, "read": 1}
    defining = {"alloc", "alias", "clone", "phi"}
    definitions = {}
    for name, block in blocks.items():
        past_phi = False
        for index, insn in enumerate(block.instructions):
            if insn.op == "phi":
                if past_phi or insn.args or not predecessors[name]:
                    raise InvalidSSA("invalid phi placement")
                incoming = [pred for pred, _ in insn.incoming]
                if (len(incoming) != len(set(incoming))
                        or set(incoming) != predecessors[name]):
                    raise InvalidSSA("phi predecessor mismatch")
            else:
                past_phi = True
                if (insn.op not in arities or insn.incoming
                        or len(insn.args) != arities[insn.op]):
                    raise InvalidSSA("unknown opcode or invalid operands")
            if bool(insn.dest) != (insn.op in defining):
                raise InvalidSSA("invalid destination")
            if insn.dest:
                if insn.dest in definitions:
                    raise InvalidSSA("duplicate definition: " + insn.dest)
                definitions[insn.dest] = (name, index)

    def definition(value):
        if value not in definitions:
            raise InvalidSSA("undefined value: " + value)
        return definitions[value]

    for name, block in blocks.items():
        for index, insn in enumerate(block.instructions):
            for value in insn.args:
                site, position = definition(value)
                if (site not in dominators[name]
                        or (site == name and position >= index)):
                    raise InvalidSSA("non-dominating use: " + value)
            for predecessor, value in insn.incoming:
                site, _ = definition(value)
                if site not in dominators[predecessor]:
                    raise InvalidSSA("non-dominating phi edge: " + value)
    return dominators


@dataclass(frozen=True)
class Execution:
    violations: tuple
    allocations: int
    consumptions: int


def execute(graph, path):
    """Interpret a finite CFG path with explicit storage identity.

    Each dynamic alloc/clone creates fresh storage. Alias/phi preserve it.
    A consume transfers ownership away from this execution. Any subsequent
    access through any name for that storage violates ownership.
    """
    verify(graph)
    if not path or path[0] != graph.entry:
        raise ValueError("path must start at entry")
    for previous, name in zip(path, path[1:]):
        if name not in graph.blocks[previous].successors:
            raise ValueError("path follows a missing edge")
    if graph.blocks[path[-1]].successors:
        raise ValueError("path must terminate at an exit")
    values, consumed = {}, set()
    allocations = consumptions = 0
    predecessor = None
    for name in path:
        block = graph.blocks[name]
        # Parallel phi assignment reads the preceding dynamic environment.
        phis = {}
        for insn in block.instructions:
            if insn.op == "phi":
                phis[insn.dest] = values[dict(insn.incoming)[predecessor]]
        values.update(phis)
        for insn in block.instructions:
            if insn.op == "phi":
                continue
            operands = [values[arg] for arg in insn.args]
            if any(resource in consumed for resource in operands):
                message = (f"{name}: {insn.op} of consumed storage "
                           f"through {insn.args[0]}")
                return Execution((message,), allocations, consumptions)
            if insn.op in ("alloc", "clone"):
                values[insn.dest] = allocations
                allocations += 1
            elif insn.op == "alias":
                values[insn.dest] = operands[0]
            elif insn.op == "consume":
                consumed.add(operands[0])
                consumptions += 1
        predecessor = name
    return Execution((), allocations, consumptions)


def loop(fresh=False):
    if fresh:
        return Graph({
            "entry": Block((), ("header",)),
            "header": Block((), ("body", "exit")),
            "body": Block((Instruction("alloc", "local0"),
                           Instruction("consume", args=("local0",))),
                          ("header",)),
            "exit": Block(),
        })
    return Graph({
        "entry": Block((Instruction("alloc", "map0"),), ("header",)),
        "header": Block((Instruction("phi", "map1", incoming=(
            ("entry", "map0"), ("body", "map1"))),), ("body", "exit")),
        "body": Block((Instruction("consume", args=("map1",)),),
                      ("header",)),
        "exit": Block(),
    })


def alias(cloned=False):
    return Graph({"entry": Block((
        Instruction("alloc", "map0"),
        Instruction("clone" if cloned else "alias", "peer0", ("map0",)),
        Instruction("consume", args=("map0",)),
        Instruction("write", args=("peer0",)),
    ))})


TWO_ITERATIONS = ("entry", "header", "body", "header", "body", "header", "exit")


def experiment():
    cases = (("F1_loop_reconsume", loop(), TWO_ITERATIONS, True),
             ("F1_fresh_control", loop(True), TWO_ITERATIONS, False),
             ("F2_alias_write", alias(), ("entry",), True),
             ("F2_clone_control", alias(True), ("entry",), False))
    results = {}
    for label, graph, path, expected in cases:
        verify(graph)
        outcome = execute(graph, path)
        if bool(outcome.violations) != expected:
            raise AssertionError((label, outcome))
        results[label] = {"ssa_valid": True,
                          "violations": list(outcome.violations),
                          "allocations": outcome.allocations,
                          "consumptions": outcome.consumptions}
    return results


if __name__ == "__main__":
    import json
    print(json.dumps(experiment(), indent=2, sort_keys=True))
