Contributor details
Your Expensify account email: 3055073676@qq.com
Upwork Profile Link: https://www.upwork.com/freelancers/~01a6c3832b8108493d

## Proposal

### Please re-state the problem that we are trying to solve in this issue.

In Go-Hybrid, all admitted computations currently lower directly into ordinary Go functions, goroutines, and control flow. As established in the general liveness limitation of `GUARANTEES.md`, Go-Hybrid guarantees memory safety, nil-safety, and cross-goroutine sendability, but provides no language-level execution bounding or termination contract. A computation executed by the Go backend runs with unrestricted wall-clock duration, unbounded Go stack growth (`runtime.morestack`), and ambient access to the Go runtime. 

When running untrusted, tenant-supplied, or resource-constrained logic, callers currently have only cooperative cancellation (such as polling `ctx.Done()`). However:
1. Cooperative cancellation cannot preempt tight uncooperative loops, non-allocating routines, or compute-heavy pure algorithms.
2. Naive loop-counter rewrites in emitted Go fail because hidden callees, mutual recursion, bulk memory operations (`copy`, slice reslices, string conversions), and internal panic recovery (`recover()`) can bypass or reset the counter.
3. Heavyweight alternatives (e.g., spinning up isolated Wasmtime instances per task as in W-019/#462) incur substantial cold-start, memory isolation, and FFI overhead that is excessive for pure, language-level algorithmic tasks.

The goal is to introduce an opt-in **compute capsule** (`capsule fn` or `@capsule`) with a non-renewable, per-invocation logical fuel budget. The capsule enforces a closed call graph over a restricted, pure language subset (fixed-width scalars, bounded byte buffers, pure functions; rejecting ambient Go imports, callbacks, channels/tasks, Rust bridges, and `managed` types). The compiler lowers this closed graph into an explicit Go-emitted control-flow machine featuring a single dispatcher, explicit bounded call frames, and strictly positive checked debits before every admitted transition and bulk operation. Execution terminates deterministically with either the computed value or `BudgetExceeded`.

---

### What is the root cause of that problem?

The lack of deterministic, bounded execution stems from three core architectural assumptions in the Go-Hybrid pipeline:

1. **Unrestricted 1:1 Go Function Lowering (`compiler/crates/hy-emit/src/func.rs`, `stmt.rs`)**:
   `hy-emit` lowers every Hybrid AST function (`FnDecl`) directly into a native Go function (`func name(...) { ... }`). Control-flow primitives (`for`, `if`, match) lower to native Go keywords without any state tracking, cycle accounting, or instruction counter injection. Execution cannot be halted externally without OS-level process termination or cooperative runtime checks.

2. **Absence of a Hermetic Call-Graph Admission Pass (`compiler/crates/hy-check/src/buildable.rs`, `compiler/crates/hy-check/src/foreign.rs`)**:
   `hy-check` validates type validity, nil safety (N1–N4 in `sendability.rs`), and buildability (`check_buildable`), but admits ambient Go calls (`foreign.rs`), Rust bridge invocations (`is_rust_call`), task/channel concurrency (`Expr::Send`, `Expr::Recv`, `Stmt::Go`), and dynamically allocated structures (`managed`). There is no admission pass to prove that a call graph is closed, pure, and free of ambient side effects.

3. **Unbounded Native Call Stack Recursion (`compiler/crates/hy-emit/src/expr.rs`)**:
   Function calls are emitted as native Go calls (`CallExpr`). When recursion occurs, the Go runtime dynamically allocates new stack segments. There is no mechanism to enforce an explicit call frame budget or prevent stack exhaustion.

4. **Lack of Pre-Charge Accounting on Variable-Sized Operations**:
   Operations such as byte-slice comparisons, copies, and transformations perform unbounded iteration in Go runtime primitives before any instruction-level counter can intercede.

---

### What changes do you think we should make in order to solve the problem?

We propose implementing the compute capsule feature through a four-stage pipeline:
1. **AST & Syntax**: Capsule declaration and budget annotations in `hy-syntax`.
2. **Admission Checker**: Closed call-graph verification, purity analysis, and type domain enforcement in `hy-check`.
3. **Versioned Cost Model**: Static logical debit assignment with checked pre-charge semantics.
4. **Machine Lowering & Go Dispatcher**: Flat bytecode/transition machine emission with explicit frame stacks and non-renewable budgets in `hy-emit`.

#### 1. Syntax & AST Support (`compiler/crates/hy-syntax`)

In `compiler/crates/hy-syntax/src/ast.rs`, add the `Capsule` declaration variant and expression:

```rust
// compiler/crates/hy-syntax/src/ast.rs

#[derive(Debug, Clone, PartialEq)]
pub struct CapsuleDecl {
    pub name: Ident,
    pub params: Vec<Param>,
    pub return_type: Option<TypeExpr>,
    pub body: Block,
    pub span: Span,
}

pub enum Decl {
    // Existing variants...
    Fn(FnDecl),
    Capsule(CapsuleDecl),
}
```

In `compiler/crates/hy-syntax/src/parser.rs`, parse `capsule fn <name>(<args>) <ret> { <body> }`.

#### 2. Closed Call-Graph Admission Pass (`compiler/crates/hy-check/src/capsule.rs`)

Introduce `compiler/crates/hy-check/src/capsule.rs` to validate the capsule's boundary conditions:
- **Type Domain Restriction**: Admitted types are strictly limited to fixed-width scalars (`int`, `int64`, `int32`, `uint64`, `uint32`, `float64`, `bool`), bounded arrays (`[N]byte`), and bounded byte slices with static maximum capacities. Types such as `managed`, pointers (`*T`), dynamic maps, channels, and arbitrary structs with pointer fields are rejected with `HYB-E5001`.
- **Closed Call Graph**: All function calls from within a capsule must resolve to other pure capsule functions declared within the same compilation unit.
- **Forbidden Constructs**: Rejects ambient Go imports, callbacks, closures capturing mutable references, goroutine spawning (`go`), channel operations (`<-`, `chan`), and Rust bridge calls (`rust/`).

```rust
// compiler/crates/hy-check/src/capsule.rs

use hy_diag::{Diagnostic, Span};
use hy_syntax::*;
use std::collections::{HashSet, HashMap};

pub struct CapsuleAdmissionChecker<'a> {
    pub diagnostics: Vec<Diagnostic>,
    pub valid_capsule_callees: HashSet<String>,
    pub call_graph: HashMap<String, Vec<(String, Span)>>,
}

impl<'a> CapsuleAdmissionChecker<'a> {
    pub fn check_capsule(&mut self, decl: &CapsuleDecl) {
        // 1. Verify parameter and return types
        for param in &decl.params {
            self.verify_admitted_type(&param.ty, param.span);
        }
        if let Some(ref ret) = decl.return_type {
            self.verify_admitted_type(ret, decl.span);
        }

        // 2. Traverse statements and expressions for closed purity
        self.check_block(&decl.body);
    }

    fn verify_admitted_type(&mut self, ty: &TypeExpr, span: Span) {
        match ty {
            TypeExpr::Ident(name) if matches!(name.as_str(), "int" | "int64" | "int32" | "uint64" | "uint32" | "float64" | "bool" | "byte") => {}
            TypeExpr::Array { element, .. } => self.verify_admitted_type(element, span),
            _ => {
                self.diagnostics.push(Diagnostic::error(
                    span,
                    format!("type `{}` is not permitted inside a compute capsule (HYB-E5001)", ty.render()),
                ));
            }
        }
    }

    fn check_expr(&mut self, expr: &Expr) {
        match expr {
            Expr::Call { callee, args, span } => {
                if let Expr::Ident(name) = &**callee {
                    if !self.valid_capsule_callees.contains(name) {
                        self.diagnostics.push(Diagnostic::error(
                            *span,
                            format!("call to non-capsule / ambient function `{}` is rejected in compute capsule (HYB-E5002)", name),
                        ));
                    }
                } else {
                    self.diagnostics.push(Diagnostic::error(
                        *span,
                        "dynamic function pointers and closures are rejected inside compute capsules (HYB-E5002)",
                    ));
                }
                for arg in args {
                    self.check_expr(&arg.value);
                }
            }
            Expr::Send { span, .. } | Expr::Recv { span, .. } => {
                self.diagnostics.push(Diagnostic::error(
                    *span,
                    "channel concurrency primitives are strictly prohibited in compute capsules (HYB-E5003)",
                ));
            }
            _ => { /* recurse children */ }
        }
    }
}
```

#### 3. Versioned Cost Model (`CapsuleCostModelV1`)

Define explicit, deterministic logical debits in `compiler/crates/hy-check/src/cost_model.rs`:
- Every basic block transition / branch: **1 unit**.
- Function call / frame push: **2 units**.
- Fixed-width scalar arithmetic/logical operations: **1 unit**.
- Variable-size operations (`copy`, slice search, slice equality): **`1 + ceil(len / 64)` units**, charged **strictly prior** to execution.
- Bounded call frame depth: `MAX_CAPSULE_FRAMES = 256`.

**Proof of Invariants**:
- *Non-increasing budget*: No transition or operation can credit or reset the budget. `debit(n)` strictly subtracts `n`.
- *No underflow/overflow*: Debits are verified using `if fuel < cost { return BudgetExceeded }` before decrementing. Fuel is stored as an unsigned 64-bit integer (`uint64`).
- *Strictly positive cycle cost*: Every closed-loop cycle contains at least one basic-block transition with `cost >= 1`. An infinite loop with finite initial budget $B$ is guaranteed to exit in at most $B$ transitions.

#### 4. Lowering to Go Control-Flow Machine (`compiler/crates/hy-emit/src/capsule.rs`)

Capsule functions do not emit as standard Go functions. Instead, each capsule lowers into a Go-emitted state machine with an explicit execution stack, a single switch-case dispatcher, and a non-renewable fuel debit:

```go
// Generated Go Prelude / Runtime Machine for Capsules
package runtime_capsule

import "errors"

var (
    ErrBudgetExceeded = errors.New("capsule: logical fuel budget exceeded")
    ErrStackOverflow  = errors.New("capsule: frame stack limit exceeded")
)

const MAX_FRAMES = 256

type Frame struct {
    ReturnPC int
    Locals   [16]uint64
}

type Machine struct {
    Fuel   uint64
    PC     int
    SP     int
    Frames [MAX_FRAMES]Frame
    Memory [65536]byte
}

func (m *Machine) Debit(cost uint64) bool {
    if m.Fuel < cost {
        m.Fuel = 0
        return false
    }
    m.Fuel -= cost
    return true
}
```

Lowered capsule function structure emitted in Go:

```go
// Emitted code for: capsule fn fib(n int) int
func HyCapsule_fib(initialFuel uint64, arg0 int64) (int64, error) {
    m := runtime_capsule.Machine{
        Fuel: initialFuel,
        PC:   0,
        SP:   0,
    }
    // Set initial frame parameters
    m.Frames[0].Locals[0] = uint64(arg0)

    for {
        // Transition debit: strictly positive before every step
        if !m.Debit(1) {
            return 0, runtime_capsule.ErrBudgetExceeded
        }

        switch m.PC {
        case 0: // entry: if n <= 1 return n
            n := int64(m.Frames[m.SP].Locals[0])
            if n <= 1 {
                m.Frames[m.SP].Locals[1] = uint64(n)
                m.PC = 999 // return block
                continue
            }
            m.PC = 1
        case 1: // Call fib(n - 1)
            if m.SP >= runtime_capsule.MAX_FRAMES-1 {
                return 0, runtime_capsule.ErrStackOverflow
            }
            if !m.Debit(2) { // Frame allocation debit
                return 0, runtime_capsule.ErrBudgetExceeded
            }
            n := int64(m.Frames[m.SP].Locals[0])
            m.SP++
            m.Frames[m.SP].ReturnPC = 2
            m.Frames[m.SP].Locals[0] = uint64(n - 1)
            m.PC = 0 // jump to entry
        case 2: // Return from fib(n - 1), call fib(n - 2)
            res1 := m.Frames[m.SP+1].Locals[1]
            m.Frames[m.SP].Locals[2] = res1 // store intermediate
            if m.SP >= runtime_capsule.MAX_FRAMES-1 {
                return 0, runtime_capsule.ErrStackOverflow
            }
            if !m.Debit(2) {
                return 0, runtime_capsule.ErrBudgetExceeded
            }
            n := int64(m.Frames[m.SP].Locals[0])
            m.SP++
            m.Frames[m.SP].ReturnPC = 3
            m.Frames[m.SP].Locals[0] = uint64(n - 2)
            m.PC = 0
        case 3: // Collect both results and return
            res1 := int64(m.Frames[m.SP].Locals[2])
            res2 := int64(m.Frames[m.SP+1].Locals[1])
            m.Frames[m.SP].Locals[1] = uint64(res1 + res2)
            m.PC = 999
        case 999: // Function epilogue
            if m.SP == 0 {
                return int64(m.Frames[0].Locals[1]), nil
            }
            retPC := m.Frames[m.SP].ReturnPC
            m.SP--
            m.PC = retPC
        default:
            panic("capsule: illegal program counter")
        }
    }
}
```

---

### What alternative solutions did you explore? (Optional)

1. **Wasmtime / Wasm Instance Sandbox (W-019 / #462)**:
   - *Exploration*: Compiling capsule code to WebAssembly and instantiating a Wasmtime runtime with engine fuel interruption.
   - *Why rejected*: Wasmtime incurs heavy CGO/Rust FFI crossing costs and requires per-task runtime allocations. It addresses memory isolation rather than language-level bounded accounting. Our single Go dispatcher achieves zero-FFI overhead, builds cleanly as native Go, and requires no external binary dependencies.

2. **AST-Level Source Instrumentation (Injecting Loop Counters into Standard Go)**:
   - *Exploration*: Adding `fuel--; if fuel <= 0 { return err }` before ordinary Go `for` loops and functions.
   - *Why rejected*: Fails soundness invariants. Native Go function recursion consumes unchecked OS/goroutine stack space; ambient Go callees cannot be metered; and Go's `recover()` could intercept budget exhaustion and resume execution. An explicit dispatcher with managed frames eliminates all stack and recovery bypasses.

3. **OS Threads / Preemption Signals / `context.Context`**:
   - *Exploration*: Running unmetered Go code in a separate goroutine and terminating via `context.WithTimeout`.
   - *Why rejected*: Relies on non-deterministic wall-clock time rather than a logical cost model. Tight non-allocating loops can delay Go runtime preemption checkpoints indefinitely.

---

### What automated tests can you add to prevent this from re-occurring?

Add the following suite to `compiler/crates/hy-check/tests/capsule_metering.rs` and `compiler/crates/hy-emit/tests/capsule_machine.rs`:

```rust
// compiler/crates/hy-check/tests/capsule_metering.rs

#[test]
fn test_admission_rejects_ambient_calls_and_concurrency() {
    // 1. Rejects ambient Go imports and foreign calls
    let r1 = check(r#"
capsule fn bad_io() int {
    println("ambient print") // Rejected: ambient call
    return 42
}
"#);
    assert_eq!(r1.verdict, Verdict::Reject);
    assert!(codes(&r1).contains(&"HYB-E5002"));

    // 2. Rejects task/channel operations
    let r2 = check(r#"
capsule fn bad_concurrency(ch chan int) int {
    go bad_io() // Rejected: goroutine spawn
    return <-ch // Rejected: channel receive
}
"#);
    assert_eq!(r2.verdict, Verdict::Reject);
    assert!(codes(&r2).contains(&"HYB-E5003"));

    // 3. Rejects pointers and managed types
    let r3 = check(r#"
capsule fn bad_type(x *int) int {
    return *x // Rejected: pointer dereference
}
"#);
    assert_eq!(r3.verdict, Verdict::Reject);
    assert!(codes(&r3).contains(&"HYB-E5001"));
}

#[test]
fn test_infinite_loop_fuel_exhaustion() {
    let src = r#"
capsule fn spin_forever() int {
    var x = 0
    for {
        x = x + 1
    }
    return x
}
"#;
    let (emitted_go, check_res) = compile_to_go(src);
    assert_eq!(check_res.verdict, Verdict::Accept);
    assert!(emitted_go.contains("runtime_capsule.ErrBudgetExceeded"));

    // Run emitted Go: Must terminate with ErrBudgetExceeded and not hang
    let go_test = r#"
func TestInfiniteLoopTerminates(t *testing.T) {
    _, err := HyCapsule_spin_forever(1000)
    if !errors.Is(err, runtime_capsule.ErrBudgetExceeded) {
        t.Fatalf("expected ErrBudgetExceeded, got: %v", err)
    }
}
"#;
    run_go_test(&emitted_go, go_test);
}

#[test]
fn test_mutual_recursion_frame_bounding() {
    let src = r#"
capsule fn ping(n int) int {
    if n <= 0 { return 0 }
    return pong(n - 1)
}
capsule fn pong(n int) int {
    if n <= 0 { return 0 }
    return ping(n - 1)
}
"#;
    let (emitted_go, check_res) = compile_to_go(src);
    assert_eq!(check_res.verdict, Verdict::Accept);

    let go_test = r#"
func TestRecursionBounding(t *testing.T) {
    // A: High fuel but recursion depth > MAX_FRAMES (256) -> ErrStackOverflow
    _, err := HyCapsule_ping(50000, 500)
    if !errors.Is(err, runtime_capsule.ErrStackOverflow) {
        t.Fatalf("expected ErrStackOverflow, got: %v", err)
    }

    // B: Insufficient fuel for depth -> ErrBudgetExceeded before stack overflow
    _, errFuel := HyCapsule_ping(10, 500)
    if !errors.Is(errFuel, runtime_capsule.ErrBudgetExceeded) {
        t.Fatalf("expected ErrBudgetExceeded, got: %v", errFuel)
    }
}
"#;
    run_go_test(&emitted_go, go_test);
}

#[test]
fn test_exact_fuel_boundary_success() {
    let src = r#"
capsule fn add(a int, b int) int {
    return a + b
}
"#;
    let (emitted_go, check_res) = compile_to_go(src);
    assert_eq!(check_res.verdict, Verdict::Accept);

    let go_test = r#"
func TestExactBoundary(t *testing.T) {
    // 1 transition (1) + return (1) = 2 fuel
    res, err := HyCapsule_add(2, 10, 20)
    if err != nil || res != 30 {
        t.Fatalf("expected success with exact fuel, got res=%v, err=%v", res, err)
    }

    // 1 fuel: insufficient to complete
    _, errFail := HyCapsule_add(1, 10, 20)
    if !errors.Is(errFail, runtime_capsule.ErrBudgetExceeded) {
        t.Fatalf("expected ErrBudgetExceeded on fuel=1, got: %v", errFail)
    }
}
"#;
    run_go_test(&emitted_go, go_test);
}

#[test]
fn test_bulk_bytes_pre_charge_charge_before_work() {
    let src = r#"
capsule fn check_bytes(src [128]byte, needle byte) bool {
    var i = 0
    for i < 128 {
        if src[i] == needle { return true }
        i = i + 1
    }
    return false
}
"#;
    let (emitted_go, check_res) = compile_to_go(src);
    assert_eq!(check_res.verdict, Verdict::Accept);

    // Negative control: Fuel is exhausted before inner scan completes
    let go_test = r#"
func TestPreChargeBytes(t *testing.T) {
    var buf [128]byte
    buf[127] = 0xFF
    _, err := HyCapsule_check_bytes(10, buf, 0xFF)
    if !errors.Is(err, runtime_capsule.ErrBudgetExceeded) {
        t.Fatalf("expected ErrBudgetExceeded before scan finished, got: %v", err)
    }
}
"#;
    run_go_test(&emitted_go, go_test);
}
```