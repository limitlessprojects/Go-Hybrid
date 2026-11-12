#!/usr/bin/env python3
"""
ssa_compiler.py — Executable SSA Lowering Prototype for W-3888 Spike.
Demonstrates direct AST-to-SSA basic block and instruction synthesis
bypassing textual Go source code emission.
"""

import sys, os, time, json, argparse
from dataclasses import dataclass, field
from typing import List, Dict, Optional, Tuple, Any

@dataclass
class SSAInstruction:
    op: str
    args: List[str]
    result: Optional[str] = None
    comment: Optional[str] = None

    def render(self) -> str:
        res = f"{self.result} = " if self.result else ""
        args_str = ", ".join(self.args)
        comm = f"  // {self.comment}" if self.comment else ""
        return f"    {res}{self.op}({args_str}){comm}"

@dataclass
class SSABasicBlock:
    label: str
    instructions: List[SSAInstruction] = field(default_factory=list)
    predecessors: List[str] = field(default_factory=list)
    successors: List[str] = field(default_factory=list)

    def add_instr(self, op: str, args: List[str], result: Optional[str] = None, comment: Optional[str] = None) -> SSAInstruction:
        instr = SSAInstruction(op=op, args=args, result=result, comment=comment)
        self.instructions.append(instr)
        return instr

@dataclass
class SSAFunction:
    name: str
    params: List[Tuple[str, str]]
    results: List[str]
    blocks: List[SSABasicBlock] = field(default_factory=list)

    def new_block(self, label: str) -> SSABasicBlock:
        b = SSABasicBlock(label=label)
        self.blocks.append(b)
        return b

    def dump_ir(self) -> str:
        lines = [f"define {self.name}({', '.join(f'{n}: {t}' for n, t in self.params)}) -> ({', '.join(self.results)}):"]
        for b in self.blocks:
            lines.append(f"{b.label}:")
            for instr in b.instructions:
                lines.append(instr.render())
        return "\n".join(lines)

class SSALoweringEngine:
    def __init__(self):
        self.var_counter = 0

    def next_var(self, prefix: str = "t") -> str:
        self.var_counter += 1
        return f"{prefix}{self.var_counter}"

    def lower_pipeline(self, fn_name: str, has_task_spawn: bool = True) -> SSAFunction:
        """Lowers an idiomatic concurrent pipeline into Go SSA IR with EscNone annotations."""
        fn = SSAFunction(name=fn_name, params=[("jobs", "int"), ("out", "chan int")], results=["error"])
        
        # Block 0: entry
        b_entry = fn.new_block("entry")
        v_ch = self.next_var("ch")
        b_entry.add_instr("MakeChan", ["int", "10"], result=v_ch, comment="buffered task channel")
        
        # Linear ownership allocation with escape certification
        v_buf = self.next_var("stack_buf")
        b_entry.add_instr("Alloc", ["sizeof([1024]byte)"], result=v_buf, comment="hy:esc_none Tier-0 stack scratch")
        
        if has_task_spawn:
            # Spawn task worker
            v_worker = self.next_var("fn_ref")
            b_entry.add_instr("FunctionRef", ["worker_task"], result=v_worker)
            b_entry.add_instr("Go", [v_worker, v_ch, v_buf], comment="task worker(move buf, ch)")
        
        b_entry.add_instr("Jump", ["loop.head"])
        
        # Block 1: loop.head
        b_loop = fn.new_block("loop.head")
        b_loop.predecessors.append("entry")
        v_i = self.next_var("i")
        b_loop.add_instr("Phi", ["0", "i.next"], result=v_i)
        v_cond = self.next_var("cond")
        b_loop.add_instr("BinOp", ["<", v_i, "jobs"], result=v_cond)
        b_loop.add_instr("If", [v_cond, "loop.body", "exit"])
        
        # Block 2: loop.body
        b_body = fn.new_block("loop.body")
        b_body.predecessors.append("loop.head")
        b_body.add_instr("Send", [v_ch, v_i])
        v_inc = self.next_var("i.next")
        b_body.add_instr("BinOp", ["+", v_i, "1"], result=v_inc)
        b_body.add_instr("Jump", ["loop.head"])
        
        # Block 3: exit
        b_exit = fn.new_block("exit")
        b_exit.predecessors.append("loop.head")
        b_exit.add_instr("Close", [v_ch])
        b_exit.add_instr("Return", ["nil"])
        
        return fn

def run_benchmark(iterations: int = 5000) -> Dict[str, Any]:
    """Measures SSA direct in-memory IR generation vs textual Go string generation + parsing."""
    engine = SSALoweringEngine()
    
    # 1. Direct SSA Lowering
    t0 = time.perf_counter()
    for _ in range(iterations):
        fn = engine.lower_pipeline("pipeline_worker", has_task_spawn=True)
        ir = fn.dump_ir()
    t_ssa = time.perf_counter() - t0
    
    # 2. Textual Transpilation (Simulated: String building + formatting + indent parsing)
    t1 = time.perf_counter()
    for _ in range(iterations):
        lines = []
        lines.append("package main")
        lines.append("func pipeline_worker(jobs int, out chan int) error {")
        lines.append("    ch := make(chan int, 10)")
        lines.append("    var stack_buf [1024]byte")
        lines.append("    go worker_task(stack_buf[:], ch)")
        lines.append("    for i := 0; i < jobs; i++ {")
        lines.append("        ch <- i")
        lines.append("    }")
        lines.append("    close(ch)")
        lines.append("    return nil")
        lines.append("}")
        text = "\n".join(lines)
        # Parse pass simulating go/parser token validation
        tokens = text.split()
        for tok in tokens:
            _ = len(tok)
    t_text = time.perf_counter() - t1
    
    speedup = (t_text - t_ssa) / t_text * 100.0 if t_text > 0 else 0.0
    return {
        "iterations": iterations,
        "direct_ssa_time_sec": round(t_ssa, 4),
        "textual_pipeline_time_sec": round(t_text, 4),
        "speedup_percentage": round(speedup, 2),
        "speedup_factor": round(t_text / t_ssa if t_ssa > 0 else 1.0, 2),
        "pass_h1_threshold": speedup >= 45.0
    }

def main():
    parser = argparse.ArgumentParser(description="W-3888 SSA Lowering Spike Verifier")
    parser.add_argument("--bench", action="store_true", help="Run comparative compilation throughput benchmark (H1)")
    parser.add_argument("--dump-ir", action="store_true", help="Dump generated SSA IR for inspection")
    parser.add_argument("--json", action="store_true", help="Output benchmark results in JSON format")
    args = parser.parse_args()

    engine = SSALoweringEngine()
    fn = engine.lower_pipeline("pipeline_stage", has_task_spawn=True)

    if args.dump_ir:
        print("=== Generated Go SSA IR (W-3888 Spike) ===")
        print(fn.dump_ir())

    if args.bench:
        res = run_benchmark(iterations=10000)
        if args.json:
            print(json.dumps(res, indent=2))
        else:
            print(f"=== Compilation Throughput Benchmark (H1 Gate) ===")
            print(f"Iterations: {res['iterations']}")
            print(f"Direct SSA Synthesis Time: {res['direct_ssa_time_sec']}s")
            print(f"Textual Transpilation Time: {res['textual_pipeline_time_sec']}s")
            print(f"Wall-Clock Reduction: {res['speedup_percentage']}% (Factor: {res['speedup_factor']}x)")
            print(f"H1 Falsifier Verdict: {'PASS (≥ 45% reduction verified)' if res['pass_h1_threshold'] else 'FAIL'}")

if __name__ == "__main__":
    main()
