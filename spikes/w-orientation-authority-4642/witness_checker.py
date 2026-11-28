"""W-4642 static half: an indexed turn type, checked intraprocedurally.

The runtime gate in ``orientation_authority.py`` proves that *if* every write
goes through ``Mesh.commit``, a misbound witness is caught.  It does not stop
the two things the proposal actually asks the language to stop:

  * a caller that never produces a witness at all (it carries a bare sign), and
  * a witness that is silently re-associated across a helper boundary.

This module implements the rule as a type system over a tiny Hybrid-shaped IR
and reports what it costs.  The rule:

    observe(m, i, j, k)   : obs[m@e, (i, j, k)]
    orientation(o)        : turn[m@e, (i, j, k)]
    insert(m, i, j, k, t) : requires t : turn[m@e_now, (i, j, k)]
    edit(m)               : e -> e+1; every turn[m@e_old, _] becomes stale
    f(...) -> turn[m@m,(i, j, k)]  is the only way a helper may hand a witness
                                   across a boundary; ``@m`` is the epoch of the
                                   caller's mesh at the call site, so the callee
                                   must not move that epoch (checked as an
                                   effect: no ``edit`` of a passed mesh).

An unindexed ``turn`` (a bare "some witness") therefore unifies with nothing at
a write site -- which is the point of the proposal.

Scope, stated up front: straight-line statements, ``return``-by-signature, and
cross-function calls.  No control flow, no loops, no generics, no method
receivers, no escaping closures.  Nothing here claims to be a front-end.
"""

from __future__ import annotations

import re
from dataclasses import dataclass, field

# ---------------------------------------------------------------------------
# Types
# ---------------------------------------------------------------------------


@dataclass(frozen=True)
class MeshTy:
    name: str

    def __str__(self) -> str:
        return f"mesh[{self.name}]"


@dataclass(frozen=True)
class IntTy:
    def __str__(self) -> str:
        return "int"


@dataclass(frozen=True)
class ObsTy:
    mesh: str
    epoch: int
    triple: tuple[str, str, str]

    def __str__(self) -> str:
        return f"obs[{self.mesh}@{self.epoch},({','.join(self.triple)})]"


@dataclass(frozen=True)
class TurnTy:
    """A witness type.  ``mesh is None`` is the unindexed form ``turn``."""

    mesh: str | None = None
    epoch: int | None = None
    triple: tuple[str, str, str] | None = None

    def __str__(self) -> str:
        if self.mesh is None:
            return "turn"
        return f"turn[{self.mesh}@{self.epoch},({','.join(self.triple)})]"


# ---------------------------------------------------------------------------
# Diagnostics
# ---------------------------------------------------------------------------

E_MESH = "HYB-E4642"     # witness bound to a different mesh
E_STALE = "HYB-E4643"    # witness bound to an epoch the mesh has left
E_TRIPLE = "HYB-E4644"   # witness bound to a different ordered triple
E_UNBOUND = "HYB-E4645"  # witness with no indices at a write site
E_SHAPE = "HYB-E4646"    # malformed input
E_MOVED = "HYB-E4647"    # callee may move the epoch of a mesh it was given


class CheckError(Exception):
    def __init__(self, code: str, message: str) -> None:
        super().__init__(f"{code}: {message}")
        self.code = code
        self.message = message


# ---------------------------------------------------------------------------
# AST
# ---------------------------------------------------------------------------


@dataclass
class Function:
    name: str
    params: list[tuple[str, str]]
    ret: str | None
    body: list[str] = field(default_factory=list)
    line: int = 0


@dataclass
class Program:
    functions: dict[str, Function] = field(default_factory=dict)
    order: list[str] = field(default_factory=list)


_FN_HEAD = re.compile(r"^fn\s+(\w+)\s*\((.*?)\)\s*(?:->\s*(.+?))?\s*\{$")
_ASSIGN = re.compile(r"^(\w+)\s*=\s*(.*)$")
_CALL = re.compile(r"^(?:call\s+)?(\w+)\s*\((.*)\)$")


def parse(source: str) -> Program:
    program = Program()
    current: Function | None = None
    for lineno, raw in enumerate(source.splitlines(), start=1):
        line = raw.split("#", 1)[0].strip()
        if not line:
            continue
        if current is None:
            head = _FN_HEAD.match(line)
            if not head:
                raise CheckError(E_SHAPE, f"line {lineno}: expected 'fn name(...) {{', got {line!r}")
            name, params_src, ret = head.group(1), head.group(2), head.group(3)
            params = []
            if params_src.strip():
                for chunk in params_src.split(","):
                    pname, ptype = chunk.split(":", 1)
                    params.append((pname.strip(), ptype.strip()))
            current = Function(name=name, params=params, ret=ret.strip() if ret else None, line=lineno)
        elif line == "}":
            program.functions[current.name] = current
            program.order.append(current.name)
            current = None
        else:
            current.body.append(line)
    if current is not None:
        raise CheckError(E_SHAPE, f"unterminated function {current.name}")
    return program


# ---------------------------------------------------------------------------
# Checker
# ---------------------------------------------------------------------------


class Checker:
    def __init__(self, program: Program) -> None:
        self.program = program

    # -- type sources -------------------------------------------------------
    @staticmethod
    def parse_type(src: str) -> MeshTy | IntTy | ObsTy | TurnTy:
        src = src.strip()
        if src == "int":
            return IntTy()
        if src == "mesh":
            return MeshTy("?")
        if src == "obs":
            return ObsTy("?", 0, ("?", "?", "?"))
        if src == "turn":
            return TurnTy()
        if src.startswith("turn["):
            match = re.fullmatch(r"turn\[(\w+)@(\w+),\((\w+),(\w+),(\w+)\)\]", src.replace(" ", ""))
            if not match:
                raise CheckError(E_SHAPE, f"malformed turn type {src!r}")
            mesh, epoch, i, j, k = match.groups()
            epoch_value: int | str = int(epoch) if epoch.isdigit() else epoch
            return TurnTy(mesh, epoch_value, (i, j, k))  # type: ignore[arg-type]
        raise CheckError(E_SHAPE, f"unknown type {src!r}")

    # -- entry point --------------------------------------------------------
    def check(self) -> None:
        for name in self.program.order:
            self.check_function(self.program.functions[name])

    def check_function(self, fn: Function) -> None:
        env: dict[str, object] = {}
        epochs: dict[str, int] = {}
        for pname, ptype_src in fn.params:
            ty = self.parse_type(ptype_src)
            if isinstance(ty, MeshTy):
                ty = MeshTy(pname)
                epochs[pname] = 1
            env[pname] = ty
        for stmt in fn.body:
            self.step(stmt, env, epochs, fn)

    # -- statements ---------------------------------------------------------
    def step(self, stmt: str, env: dict, epochs: dict[str, int], fn: Function) -> None:
        if stmt.startswith("return "):
            self.check_return(stmt[len("return "):].strip(), env, epochs, fn)
            return

        assign = _ASSIGN.match(stmt)
        target = None
        expr = stmt
        if assign:
            target, expr = assign.group(1), assign.group(2).strip()

        call = _CALL.match(expr)
        if not call:
            raise CheckError(E_SHAPE, f"{fn.name}: unsupported statement {stmt!r}")
        name, args_src = call.group(1), call.group(2)
        args = [a.strip() for a in args_src.split(",") if a.strip()]

        if target is None:
            if name == "insert":
                self.check_insert(args, env, epochs, fn)
                return
            if name == "edit":
                mesh = args[0]
                epochs[mesh] = epochs.get(mesh, 1) + 1
                return
            raise CheckError(E_SHAPE, f"{fn.name}: unsupported statement {stmt!r}")

        env[target] = self.eval_call(name, args, env, epochs, fn)

    def eval_call(self, name: str, args: list[str], env: dict, epochs: dict[str, int], fn: Function):
        if name == "observe":
            mesh, i, j, k = args
            if mesh not in env:
                raise CheckError(E_SHAPE, f"{fn.name}: unknown mesh {mesh}")
            return ObsTy(mesh, epochs.get(mesh, 1), (i, j, k))
        if name == "orientation":
            obs = env.get(args[0])
            if not isinstance(obs, ObsTy):
                raise CheckError(E_SHAPE, f"{fn.name}: orientation needs an observation, got {obs}")
            return TurnTy(obs.mesh, obs.epoch, obs.triple)
        if name in self.program.functions:
            return self.call_signature(name, args, env, epochs, fn)
        raise CheckError(E_SHAPE, f"{fn.name}: unknown call {name}")

    def call_signature(self, name: str, args: list[str], env: dict, epochs: dict[str, int], fn: Function):
        callee = self.program.functions[name]
        if callee.ret is None:
            raise CheckError(E_SHAPE, f"{fn.name}: {name} returns nothing")
        if len(args) != len(callee.params):
            raise CheckError(E_SHAPE, f"{fn.name}: {name} expects {len(callee.params)} args")
        declared = self.parse_type(callee.ret)

        binding: dict[str, str] = {}
        for (formal, _), actual in zip(callee.params, args):
            binding[formal] = actual

        if not isinstance(declared, TurnTy):
            raise CheckError(E_SHAPE, f"{fn.name}: unsupported return type for {name}")
        if declared.mesh is None:
            return TurnTy()

        mesh = binding.get(declared.mesh, declared.mesh)
        if mesh not in env:
            raise CheckError(E_SHAPE, f"{fn.name}: {name}'s return type names unknown mesh {mesh}")
        self.check_epoch_moves(name, binding, env, fn)
        epoch = declared.epoch
        if isinstance(epoch, str):
            epoch_sym = binding.get(epoch, epoch)
            epoch_value = epochs.get(epoch_sym, 1)
        else:
            epoch_value = epoch
        triple = tuple(binding.get(v, v) for v in declared.triple)
        return TurnTy(mesh, epoch_value, triple)  # type: ignore[arg-type]

    def check_epoch_moves(self, name: str, binding: dict[str, str], env: dict, fn: Function) -> None:
        """A callee that may ``edit`` a mesh it was given invalidates ``@m``."""
        callee = self.program.functions[name]
        passed = {binding[formal] for formal, ptype in callee.params if ptype.strip() == "mesh"}
        for stmt in callee.body:
            call = _CALL.match(stmt)
            if call and call.group(1) == "edit":
                edited = call.group(2).split(",")[0].strip()
                edited_actual = binding.get(edited, edited)
                if edited_actual in passed:
                    raise CheckError(
                        E_MOVED,
                        f"{fn.name}: {name} may edit mesh {edited_actual}; its witness epoch "
                        f"cannot be trusted at the call site",
                    )
        # Transitive: a callee that calls another epoch-moving helper.
        for stmt in callee.body:
            call = _CALL.match(stmt)
            if not call or call.group(1) not in self.program.functions:
                continue
            inner_name = call.group(1)
            inner_args = [a.strip() for a in call.group(2).split(",") if a.strip()]
            inner = self.program.functions[inner_name]
            inner_binding = dict(zip([p for p, _ in inner.params], inner_args))
            inner_binding = {k: binding.get(v, v) for k, v in inner_binding.items()}
            self.check_epoch_moves(inner_name, inner_binding, env, fn)

    def check_return(self, expr: str, env: dict, epochs: dict[str, int], fn: Function) -> None:
        """The body must produce exactly the witness its signature promises."""
        if fn.ret is None:
            raise CheckError(E_SHAPE, f"{fn.name}: returns a value but declares no return type")
        declared = self.parse_type(fn.ret)
        if not isinstance(declared, TurnTy):
            raise CheckError(E_SHAPE, f"{fn.name}: unsupported return type {fn.ret!r}")

        call = _CALL.match(expr)
        actual = self.eval_call(call.group(1), [a.strip() for a in call.group(2).split(",") if a.strip()],
                                env, epochs, fn) if call else env.get(expr)
        if not isinstance(actual, TurnTy):
            raise CheckError(E_UNBOUND, f"{fn.name}: return value is not a turn")

        if declared.mesh is None:
            return  # an unindexed helper may return anything; it proves nothing downstream
        if actual.mesh != declared.mesh:
            raise CheckError(E_MESH, f"{fn.name}: returns a witness for {actual.mesh}, signature says {declared.mesh}")
        expected_epoch = declared.epoch
        if isinstance(expected_epoch, str):
            expected_epoch = epochs.get(expected_epoch, epochs.get(expected_epoch.strip(), 1))
        if actual.epoch != expected_epoch:
            raise CheckError(
                E_STALE,
                f"{fn.name}: returns a witness for @{actual.epoch}, signature promises @{expected_epoch}",
            )
        expected_triple = tuple(declared.triple or ())
        if actual.triple != expected_triple:
            raise CheckError(
                E_TRIPLE,
                f"{fn.name}: returns a witness for ({','.join(actual.triple or ())}), "
                f"signature promises ({','.join(expected_triple)})",
            )

    def check_insert(self, args: list[str], env: dict, epochs: dict[str, int], fn: Function) -> None:
        if len(args) != 5:
            raise CheckError(E_SHAPE, f"{fn.name}: insert takes (mesh, i, j, k, turn)")
        mesh, i, j, k, witness = args
        actual = env.get(witness)
        if not isinstance(actual, TurnTy):
            raise CheckError(E_UNBOUND, f"{fn.name}: insert's witness {witness!r} is not a turn")
        if actual.mesh is None:
            raise CheckError(
                E_UNBOUND,
                f"{fn.name}: {witness!r} has the unindexed type 'turn' -- it proves no particular "
                f"mesh, epoch or order, so no write may consume it",
            )
        if actual.mesh != mesh:
            raise CheckError(E_MESH, f"{fn.name}: witness for mesh {actual.mesh}, insert on {mesh}")
        if actual.epoch != epochs.get(mesh, 1):
            raise CheckError(
                E_STALE,
                f"{fn.name}: witness for {mesh}@{actual.epoch}, mesh is at @{epochs.get(mesh, 1)}",
            )
        if actual.triple != (i, j, k):
            raise CheckError(
                E_TRIPLE,
                f"{fn.name}: witness for ({','.join(actual.triple)}), insert at ({i},{j},{k})",
            )


def check_source(source: str) -> None:
    Checker(parse(source)).check()
