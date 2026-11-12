from __future__ import annotations
from dataclasses import dataclass, field
from enum import Enum
from typing import Dict, List, Optional, Set, Tuple


class SSAOp(Enum):
    CONST_INT = "OpConst64"
    CONST_FLOAT = "OpConstFloat64"
    CONST_STR = "OpConstString"
    CONST_BOOL = "OpConstBool"
    LOCAL_ADDR = "OpLocalAddr"
    LOAD = "OpLoad"
    STORE = "OpStore"
    STORE_WB = "OpStoreWB"
    ADD = "OpAdd64"
    SUB = "OpSub64"
    MUL = "OpMul64"
    DIV = "OpDiv64"
    EQ = "OpEq"
    NEQ = "OpNeq"
    LT = "OpLt"
    PHI = "OpPhi"
    CALL = "OpCall"
    CHAN_MAKE = "OpChanMake"
    CHAN_SEND = "OpChanSend"
    CHAN_RECV = "OpChanRecv"
    SLICE_MAKE = "OpSliceMake"
    SLICE_LEN = "OpSliceLen"
    STRUCT_MAKE = "OpStructMake"
    FIELD_ADDR = "OpFieldAddr"
    BRANCH = "OpBranch"
    BRANCH_COND = "OpBranchCond"
    SELECT = "OpSelect"
    DEFER = "OpDefer"
    RECOVER = "OpRecover"
    BREAK = "OpBreak"
    CONTINUE = "OpContinue"
    RETURN = "OpReturn"


@dataclass
class SSAValue:
    id: int
    op: SSAOp
    val_type: str
    args: List[SSAValue] = field(default_factory=list)
    aux_int: Optional[int] = None
    aux_float: Optional[float] = None
    aux_str: Optional[str] = None
    stack_certified: bool = False

    def __repr__(self) -> str:
        args_str = ", ".join(f"v{a.id}" for a in self.args)
        aux = ""
        if self.aux_int is not None:
            aux = f" [{self.aux_int}]"
        elif self.aux_float is not None:
            aux = f" [{self.aux_float}]"
        elif self.aux_str is not None:
            aux = f" [{self.aux_str}]"
        cert = " [STACK_CERTIFIED]" if self.stack_certified else ""
        return f"v{self.id} = {self.op.value}{aux} ({args_str}) : {self.val_type}{cert}"


@dataclass
class SSABlock:
    name: str
    values: List[SSAValue] = field(default_factory=list)
    predecessors: List[SSABlock] = field(default_factory=list)
    successors: List[SSABlock] = field(default_factory=list)

    def add_value(self, val: SSAValue):
        self.values.append(val)

    def __repr__(self) -> str:
        preds = ", ".join(p.name for p in self.predecessors)
        succs = ", ".join(s.name for s in self.successors)
        return f"Block {self.name} (preds: [{preds}], succs: [{succs}])"


@dataclass
class FrameSlot:
    name: str
    offset: int
    size: int
    is_ptr: bool
    stack_certified: bool


class GCStackMap:
    @staticmethod
    def generate_bitmap(slots: List[FrameSlot]) -> Tuple[int, str]:
        sorted_slots = sorted(slots, key=lambda s: s.offset)
        mask = 0
        bits = []
        for i, slot in enumerate(sorted_slots):
            if slot.is_ptr:
                mask |= (1 << i)
                bits.append("1")
            else:
                bits.append("0")
        return mask, "".join(reversed(bits)) if bits else "0"

    @staticmethod
    def verify_safepoint_trace(slots: List[FrameSlot], live_ptrs: Set[str]) -> bool:
        for slot in slots:
            if slot.name in live_ptrs and not slot.is_ptr:
                return False
            if slot.is_ptr and slot.name not in live_ptrs and slot.stack_certified:
                pass
        return True


class SSAVersionAdapter:
    SUPPORTED_VERSIONS = ("1.21", "1.22", "1.26.9")

    def __init__(self, target_go_version: str = "1.26.9"):
        if target_go_version not in self.SUPPORTED_VERSIONS:
            raise ValueError(f"Unsupported Go toolchain version: {target_go_version}")
        self.target_go_version = target_go_version

    def map_op(self, op: SSAOp) -> str:
        if self.target_go_version == "1.21":
            if op == SSAOp.CONST_INT:
                return "OpConst64"
            if op == SSAOp.STORE_WB:
                return "OpStore"
        elif self.target_go_version == "1.22":
            if op == SSAOp.CONST_INT:
                return "OpConst64"
        return op.value

    def validate_abi(self, fn: SSAFunction) -> bool:
        for b in fn.blocks:
            for v in b.values:
                mapped = self.map_op(v.op)
                if not mapped:
                    return False
        return True


@dataclass
class SSAFunction:
    name: str
    blocks: List[SSABlock] = field(default_factory=list)
    entry: Optional[SSABlock] = None
    next_id: int = 1
    frame_slots: List[FrameSlot] = field(default_factory=list)

    def new_value(self, op: SSAOp, val_type: str, args: Optional[List[SSAValue]] = None,
                  aux_int: Optional[int] = None, aux_float: Optional[float] = None,
                  aux_str: Optional[str] = None, stack_certified: bool = False) -> SSAValue:
        v = SSAValue(
            id=self.next_id,
            op=op,
            val_type=val_type,
            args=args or [],
            aux_int=aux_int,
            aux_float=aux_float,
            aux_str=aux_str,
            stack_certified=stack_certified
        )
        self.next_id += 1
        return v

    def new_block(self, name: str) -> SSABlock:
        b = SSABlock(name=name)
        self.blocks.append(b)
        return b


@dataclass
class HyExpr:
    pass

@dataclass
class HyLitInt(HyExpr):
    val: int

@dataclass
class HyLitFloat(HyExpr):
    val: float

@dataclass
class HyLitStr(HyExpr):
    val: str

@dataclass
class HyLitBool(HyExpr):
    val: bool

@dataclass
class HyIdent(HyExpr):
    name: str

@dataclass
class HyBinary(HyExpr):
    op: str
    left: HyExpr
    right: HyExpr

@dataclass
class HyCall(HyExpr):
    fn_name: str
    args: List[HyExpr]

@dataclass
class HyField(HyExpr):
    base: HyExpr
    field_name: str

@dataclass
class HyChanRecv(HyExpr):
    chan: HyExpr

@dataclass
class HyStmt:
    pass

@dataclass
class HyAssign(HyStmt):
    var_name: str
    expr: HyExpr
    is_owned: bool = False
    escapes: bool = False
    is_ptr: bool = False

@dataclass
class HyIf(HyStmt):
    cond: HyExpr
    then_stmts: List[HyStmt]
    else_stmts: List[HyStmt]

@dataclass
class HyWhile(HyStmt):
    cond: HyExpr
    body: List[HyStmt]

@dataclass
class HyChanSend(HyStmt):
    chan: HyExpr
    val: HyExpr

@dataclass
class HyDefer(HyStmt):
    expr: HyExpr

@dataclass
class HyBreak(HyStmt):
    label: Optional[str] = None

@dataclass
class HyContinue(HyStmt):
    label: Optional[str] = None

@dataclass
class HySelect(HyStmt):
    cases: List[Tuple[Optional[HyStmt], List[HyStmt]]]
    default_stmts: Optional[List[HyStmt]] = None

@dataclass
class HyStructAlloc(HyStmt):
    var_name: str
    fields: List[Tuple[str, HyExpr, bool]]
    is_owned: bool = True
    escapes: bool = False

@dataclass
class HyReturn(HyStmt):
    expr: Optional[HyExpr] = None


class SSALoweringEngine:
    def __init__(self, write_barriers_active: bool = False):
        self.heap_allocations: int = 0
        self.stack_allocations: int = 0
        self.revoked_certificates: int = 0
        self.syntax_errors: int = 0
        self.write_barriers_active: bool = write_barriers_active
        self.write_barriers_count: int = 0

    def lower_function(self, name: str, stmts: List[HyStmt]) -> SSAFunction:
        fn = SSAFunction(name=name)
        entry = fn.new_block("b_entry")
        fn.entry = entry

        curr_block, _ = self._lower_stmts(fn, entry, stmts, {})
        return fn

    def _lower_stmts(self, fn: SSAFunction, block: SSABlock,
                     stmts: List[HyStmt], env: Dict[str, SSAValue]) -> Tuple[SSABlock, Dict[str, SSAValue]]:
        curr_block = block
        curr_env = dict(env)

        for stmt in stmts:
            if isinstance(stmt, HyAssign):
                val = self._lower_expr(fn, curr_block, stmt.expr, curr_env)
                is_ptr = stmt.is_ptr or val.val_type.startswith("*") or val.val_type.startswith("chan")

                if stmt.is_owned and not stmt.escapes:
                    slot_val = fn.new_value(
                        SSAOp.LOCAL_ADDR,
                        val_type=f"*{val.val_type}",
                        aux_str=stmt.var_name,
                        stack_certified=True
                    )
                    curr_block.add_value(slot_val)

                    store_op = SSAOp.STORE
                    if is_ptr and self.write_barriers_active:
                        store_op = SSAOp.STORE_WB
                        self.write_barriers_count += 1

                    store = fn.new_value(store_op, "void", args=[slot_val, val])
                    curr_block.add_value(store)
                    curr_env[stmt.var_name] = val
                    self.stack_allocations += 1

                    slot_offset = len(fn.frame_slots) * 8
                    fn.frame_slots.append(
                        FrameSlot(
                            name=stmt.var_name,
                            offset=slot_offset,
                            size=8,
                            is_ptr=is_ptr,
                            stack_certified=True
                        )
                    )
                elif stmt.is_owned and stmt.escapes:
                    self.revoked_certificates += 1
                    self.heap_allocations += 1
                    curr_env[stmt.var_name] = val
                else:
                    curr_env[stmt.var_name] = val

            elif isinstance(stmt, HyStructAlloc):
                if stmt.is_owned and not stmt.escapes:
                    struct_slot = fn.new_value(
                        SSAOp.LOCAL_ADDR,
                        val_type="*struct",
                        aux_str=stmt.var_name,
                        stack_certified=True
                    )
                    curr_block.add_value(struct_slot)
                    self.stack_allocations += 1

                    for f_name, f_expr, f_is_ptr in stmt.fields:
                        f_val = self._lower_expr(fn, curr_block, f_expr, curr_env)
                        f_addr = fn.new_value(SSAOp.FIELD_ADDR, f"*{f_val.val_type}", args=[struct_slot], aux_str=f_name)
                        curr_block.add_value(f_addr)

                        s_op = SSAOp.STORE
                        if f_is_ptr and self.write_barriers_active:
                            s_op = SSAOp.STORE_WB
                            self.write_barriers_count += 1
                        curr_block.add_value(fn.new_value(s_op, "void", args=[f_addr, f_val]))

                        fn.frame_slots.append(
                            FrameSlot(
                                name=f"{stmt.var_name}.{f_name}",
                                offset=len(fn.frame_slots) * 8,
                                size=8,
                                is_ptr=f_is_ptr,
                                stack_certified=True
                            )
                        )
                    curr_env[stmt.var_name] = struct_slot
                else:
                    self.revoked_certificates += 1
                    self.heap_allocations += 1
                    make_op = fn.new_value(SSAOp.STRUCT_MAKE, "struct", aux_str=stmt.var_name)
                    curr_block.add_value(make_op)
                    curr_env[stmt.var_name] = make_op

            elif isinstance(stmt, HyChanSend):
                ch_val = self._lower_expr(fn, curr_block, stmt.chan, curr_env)
                send_val = self._lower_expr(fn, curr_block, stmt.val, curr_env)
                send_op = fn.new_value(SSAOp.CHAN_SEND, "void", args=[ch_val, send_val])
                curr_block.add_value(send_op)

            elif isinstance(stmt, HyDefer):
                def_val = self._lower_expr(fn, curr_block, stmt.expr, curr_env)
                curr_block.add_value(fn.new_value(SSAOp.DEFER, "void", args=[def_val]))

            elif isinstance(stmt, HyBreak):
                curr_block.add_value(fn.new_value(SSAOp.BREAK, "void", aux_str=stmt.label))

            elif isinstance(stmt, HyContinue):
                curr_block.add_value(fn.new_value(SSAOp.CONTINUE, "void", aux_str=stmt.label))

            elif isinstance(stmt, HySelect):
                sel_block = fn.new_block(f"b_select_{curr_block.name}")
                merge_block = fn.new_block(f"b_sel_merge_{curr_block.name}")
                curr_block.successors.append(sel_block)
                sel_block.predecessors.append(curr_block)
                curr_block.add_value(fn.new_value(SSAOp.BRANCH, "void"))

                sel_op = fn.new_value(SSAOp.SELECT, "void", aux_int=len(stmt.cases))
                sel_block.add_value(sel_op)

                case_envs = []
                for i, (comm_stmt, body_stmts) in enumerate(stmt.cases):
                    c_block = fn.new_block(f"b_case_{i}_{sel_block.name}")
                    sel_block.successors.append(c_block)
                    c_block.predecessors.append(sel_block)
                    c_env = dict(curr_env)
                    if comm_stmt:
                        c_block, c_env = self._lower_stmts(fn, c_block, [comm_stmt], c_env)
                    c_exit, c_exit_env = self._lower_stmts(fn, c_block, body_stmts, c_env)
                    c_exit.successors.append(merge_block)
                    merge_block.predecessors.append(c_exit)
                    c_exit.add_value(fn.new_value(SSAOp.BRANCH, "void"))
                    case_envs.append(c_exit_env)

                if stmt.default_stmts is not None:
                    def_block = fn.new_block(f"b_default_{sel_block.name}")
                    sel_block.successors.append(def_block)
                    def_block.predecessors.append(sel_block)
                    def_exit, def_env = self._lower_stmts(fn, def_block, stmt.default_stmts, curr_env)
                    def_exit.successors.append(merge_block)
                    merge_block.predecessors.append(def_exit)
                    def_exit.add_value(fn.new_value(SSAOp.BRANCH, "void"))
                    case_envs.append(def_env)

                if case_envs:
                    all_case_vars = set()
                    for ce in case_envs:
                        all_case_vars.update(ce.keys())
                    for vname in all_case_vars:
                        vals = [ce.get(vname, curr_env.get(vname)) for ce in case_envs]
                        if any(v is None for v in vals):
                            continue
                        if len(set(v.id for v in vals)) > 1:
                            val_type = vals[0].val_type
                            phi = fn.new_value(SSAOp.PHI, val_type, args=vals, aux_str=vname)
                            merge_block.add_value(phi)
                            curr_env[vname] = phi

                curr_block = merge_block

            elif isinstance(stmt, HyIf):
                cond_val = self._lower_expr(fn, curr_block, stmt.cond, curr_env)
                then_block = fn.new_block(f"b_then_{curr_block.name}")
                else_block = fn.new_block(f"b_else_{curr_block.name}")
                merge_block = fn.new_block(f"b_merge_{curr_block.name}")

                curr_block.successors = [then_block, else_block]
                then_block.predecessors = [curr_block]
                else_block.predecessors = [curr_block]

                curr_block.add_value(fn.new_value(SSAOp.BRANCH_COND, "void", args=[cond_val]))

                then_exit, then_env = self._lower_stmts(fn, then_block, stmt.then_stmts, curr_env)
                then_exit.successors.append(merge_block)
                merge_block.predecessors.append(then_exit)
                then_exit.add_value(fn.new_value(SSAOp.BRANCH, "void"))

                else_exit, else_env = self._lower_stmts(fn, else_block, stmt.else_stmts, curr_env)
                else_exit.successors.append(merge_block)
                merge_block.predecessors.append(else_exit)
                else_exit.add_value(fn.new_value(SSAOp.BRANCH, "void"))

                all_vars = set(then_env.keys()).union(else_env.keys())
                for vname in all_vars:
                    t_val = then_env.get(vname, curr_env.get(vname))
                    e_val = else_env.get(vname, curr_env.get(vname))
                    if t_val and e_val and t_val != e_val:
                        if t_val.val_type != e_val.val_type:
                            raise TypeError(f"SSA Phi type mismatch for {vname}: {t_val.val_type} != {e_val.val_type}")
                        phi = fn.new_value(SSAOp.PHI, t_val.val_type, args=[t_val, e_val], aux_str=vname)
                        merge_block.add_value(phi)
                        curr_env[vname] = phi

                curr_block = merge_block

            elif isinstance(stmt, HyWhile):
                header_block = fn.new_block(f"b_loop_head_{curr_block.name}")
                body_block = fn.new_block(f"b_loop_body_{curr_block.name}")
                exit_block = fn.new_block(f"b_loop_exit_{curr_block.name}")

                curr_block.successors.append(header_block)
                header_block.predecessors.append(curr_block)
                curr_block.add_value(fn.new_value(SSAOp.BRANCH, "void"))

                loop_phis: Dict[str, SSAValue] = {}
                for vname, vval in curr_env.items():
                    phi = fn.new_value(SSAOp.PHI, vval.val_type, args=[vval], aux_str=vname)
                    header_block.add_value(phi)
                    loop_phis[vname] = phi

                header_env = dict(curr_env)
                header_env.update(loop_phis)

                cond_val = self._lower_expr(fn, header_block, stmt.cond, header_env)
                header_block.successors = [body_block, exit_block]
                body_block.predecessors = [header_block]
                exit_block.predecessors = [header_block]
                header_block.add_value(fn.new_value(SSAOp.BRANCH_COND, "void", args=[cond_val]))

                body_exit, body_env = self._lower_stmts(fn, body_block, stmt.body, header_env)
                body_exit.successors.append(header_block)
                header_block.predecessors.append(body_exit)
                body_exit.add_value(fn.new_value(SSAOp.BRANCH, "void"))

                for vname, phi in loop_phis.items():
                    b_val = body_env.get(vname, curr_env[vname])
                    phi.args.append(b_val)
                    curr_env[vname] = phi

                curr_block = exit_block

            elif isinstance(stmt, HyReturn):
                if stmt.expr:
                    ret_val = self._lower_expr(fn, curr_block, stmt.expr, curr_env)
                    curr_block.add_value(fn.new_value(SSAOp.RETURN, "void", args=[ret_val]))
                else:
                    curr_block.add_value(fn.new_value(SSAOp.RETURN, "void"))

        return curr_block, curr_env

    def _lower_expr(self, fn: SSAFunction, block: SSABlock,
                    expr: HyExpr, env: Dict[str, SSAValue]) -> SSAValue:
        if isinstance(expr, HyLitInt):
            val = fn.new_value(SSAOp.CONST_INT, "int64", aux_int=expr.val)
            block.add_value(val)
            return val

        elif isinstance(expr, HyLitFloat):
            val = fn.new_value(SSAOp.CONST_FLOAT, "float64", aux_float=expr.val)
            block.add_value(val)
            return val

        elif isinstance(expr, HyLitStr):
            val = fn.new_value(SSAOp.CONST_STR, "string", aux_str=expr.val)
            block.add_value(val)
            return val

        elif isinstance(expr, HyLitBool):
            val = fn.new_value(SSAOp.CONST_BOOL, "bool", aux_int=1 if expr.val else 0)
            block.add_value(val)
            return val

        elif isinstance(expr, HyIdent):
            if expr.name not in env:
                raise ValueError(f"Undefined SSA variable `{expr.name}`")
            return env[expr.name]

        elif isinstance(expr, HyBinary):
            l = self._lower_expr(fn, block, expr.left, env)
            r = self._lower_expr(fn, block, expr.right, env)
            op_map = {
                "+": SSAOp.ADD,
                "-": SSAOp.SUB,
                "*": SSAOp.MUL,
                "/": SSAOp.DIV,
                "==": SSAOp.EQ,
                "!=": SSAOp.NEQ,
                "<": SSAOp.LT,
            }
            if expr.op not in op_map:
                raise NotImplementedError(f"Unsupported binary op `{expr.op}`")
            val_type = "bool" if expr.op in ["==", "!=", "<"] else l.val_type
            val = fn.new_value(op_map[expr.op], val_type, args=[l, r])
            block.add_value(val)
            return val

        elif isinstance(expr, HyCall):
            lowered_args = [self._lower_expr(fn, block, a, env) for a in expr.args]
            ret_type = "chan[int64]" if expr.fn_name == "make_chan" else "int64" if expr.fn_name.startswith("get") else "any"
            op = SSAOp.CHAN_MAKE if expr.fn_name == "make_chan" else SSAOp.CALL
            val = fn.new_value(op, ret_type, args=lowered_args, aux_str=expr.fn_name)
            block.add_value(val)
            return val

        elif isinstance(expr, HyField):
            base_val = self._lower_expr(fn, block, expr.base, env)
            addr = fn.new_value(SSAOp.FIELD_ADDR, f"*{base_val.val_type}", args=[base_val], aux_str=expr.field_name)
            block.add_value(addr)
            loaded = fn.new_value(SSAOp.LOAD, "int64", args=[addr], aux_str=expr.field_name)
            block.add_value(loaded)
            return loaded

        elif isinstance(expr, HyChanRecv):
            ch_val = self._lower_expr(fn, block, expr.chan, env)
            recv_val = fn.new_value(SSAOp.CHAN_RECV, "int64", args=[ch_val])
            block.add_value(recv_val)
            return recv_val

        raise NotImplementedError(f"Unsupported expression {type(expr)}")
