//! Hybrid W-019 Isolated WASM Cells — Backend Admission & Coordinator Model
//!
//! Standalone reference model verifying:
//! - HYB-E3901: WireSendable admission across `spawn cell` boundaries
//! - HYB-E3902: WASM module memory & import capability admission
//! - HYB-E3903: Persistent reactor `hy_cell_step` execution vs `_start` / re-entry
//! - Intra-cell `may_share` local borrow checking vs cross-cell snapshot isolation

pub const MAX_WIRE_DEPTH: usize = 8;
pub const MAX_WIRE_FRAME_BYTES: usize = 65_536;
pub const WASM_PAGE_BYTES: usize = 65_536;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DiagnosticCode {
    /// Non-wire-sendable type or frame across `spawn cell` boundary.
    HybE3901,
    /// WASM cell module capability or memory isolation violation.
    HybE3902,
    /// Cell execution lifecycle or re-entrancy violation.
    HybE3903,
    /// Intra-cell local alias/loan conflict (standard `may_share` violation).
    IntraCellLoanConflict,
}

impl DiagnosticCode {
    pub fn as_str(&self) -> &'static str {
        match self {
            DiagnosticCode::HybE3901 => "HYB-E3901",
            DiagnosticCode::HybE3902 => "HYB-E3902",
            DiagnosticCode::HybE3903 => "HYB-E3903",
            DiagnosticCode::IntraCellLoanConflict => "HYB-LOAN-LOCAL",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Diagnostic {
    pub code: DiagnosticCode,
    pub detail: &'static str,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WireType {
    Scalar(i64),
    ByteVec(Vec<u8>),
    OwnedTree(Vec<WireType>),
    RawPtr(usize),
    MutableSliceAlias(usize),
    ClosureWithEnv(&'static str),
    ChannelEnd(u64),
    ForeignHandle(u64),
    CyclicGraph,
    DepthExceeded,
}

impl WireType {
    pub fn encoded_bytes(&self) -> usize {
        match self {
            WireType::Scalar(_) => 9,
            WireType::ByteVec(buf) => 5 + buf.len(),
            WireType::OwnedTree(children) => {
                let sum: usize = children.iter().map(WireType::encoded_bytes).sum();
                5 + sum
            }
            _ => 8,
        }
    }
}

pub fn admit_wire_sendable(value: &WireType, depth: usize) -> Result<usize, Diagnostic> {
    if depth > MAX_WIRE_DEPTH {
        return Err(Diagnostic {
            code: DiagnosticCode::HybE3901,
            detail: "wire message nesting depth exceeds MAX_WIRE_DEPTH (8)",
        });
    }
    check_wire_variant(value, depth)?;
    let size = value.encoded_bytes();
    if size > MAX_WIRE_FRAME_BYTES {
        return Err(Diagnostic {
            code: DiagnosticCode::HybE3901,
            detail: "wire message serialized frame exceeds 64 KiB limit",
        });
    }
    Ok(size)
}

fn check_wire_variant(value: &WireType, depth: usize) -> Result<(), Diagnostic> {
    match value {
        WireType::Scalar(_) | WireType::ByteVec(_) => Ok(()),
        WireType::OwnedTree(children) => {
            for child in children {
                admit_wire_sendable(child, depth + 1)?;
            }
            Ok(())
        }
        WireType::RawPtr(_)
        | WireType::MutableSliceAlias(_)
        | WireType::ClosureWithEnv(_)
        | WireType::ChannelEnd(_)
        | WireType::ForeignHandle(_)
        | WireType::CyclicGraph
        | WireType::DepthExceeded => Err(Diagnostic {
            code: DiagnosticCode::HybE3901,
            detail: "non-WireSendable type cannot cross spawn cell boundary",
        }),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MemoryModel {
    PrivateNonSharedMemory,
    SharedLinearMemory,
    ImportedHostMemory,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportDecl {
    CoordinatorSend,
    CoordinatorRecv,
    AmbientWasiImport(&'static str),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModuleSpec {
    pub memory: MemoryModel,
    pub imports: Vec<ImportDecl>,
    pub exports_cell_step: bool,
}

pub fn admit_cell_module(spec: &ModuleSpec) -> Result<(), Diagnostic> {
    if spec.memory != MemoryModel::PrivateNonSharedMemory {
        return Err(Diagnostic {
            code: DiagnosticCode::HybE3902,
            detail: "WASM cell requires private non-shared linear memory",
        });
    }
    for imp in &spec.imports {
        if let ImportDecl::AmbientWasiImport(_) = imp {
            return Err(Diagnostic {
                code: DiagnosticCode::HybE3902,
                detail: "ambient WASI or unauthorized host import rejected",
            });
        }
    }
    if !spec.exports_cell_step {
        return Err(Diagnostic {
            code: DiagnosticCode::HybE3902,
            detail: "WASM cell must export persistent hy_cell_step reactor entrypoint",
        });
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReactorState {
    Idle,
    FrameActive,
    TerminatedOneShot,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    ReactorStep,
    OneShotStart,
    ReentrantHostCallback,
}

#[derive(Debug, Clone)]
pub struct IsolatedCell {
    pub state: ReactorState,
    pub linear_memory: Vec<u8>,
    pub arena_watermark: usize,
    pub completed_steps: u64,
}

impl IsolatedCell {
    pub fn instantiate(spec: &ModuleSpec) -> Result<Self, Diagnostic> {
        admit_cell_module(spec)?;
        Ok(Self {
            state: ReactorState::Idle,
            linear_memory: vec![0u8; WASM_PAGE_BYTES],
            arena_watermark: 0,
            completed_steps: 0,
        })
    }

    pub fn invoke_entry(&mut self, entry: EntryKind, payload: &WireType) -> Result<WireType, Diagnostic> {
        validate_entry_transition(self.state, entry)?;
        let encoded_len = admit_wire_sendable(payload, 0)?;
        self.state = ReactorState::FrameActive;
        self.arena_watermark = encoded_len;
        let reply = snapshot_clone(payload);
        self.arena_watermark = 0;
        self.completed_steps += 1;
        self.state = ReactorState::Idle;
        Ok(reply)
    }

    pub fn begin_step_frame(&mut self, payload: &WireType) -> Result<usize, Diagnostic> {
        validate_entry_transition(self.state, EntryKind::ReactorStep)?;
        let encoded_len = admit_wire_sendable(payload, 0)?;
        self.state = ReactorState::FrameActive;
        self.arena_watermark = encoded_len;
        Ok(encoded_len)
    }

    pub fn finish_step_frame(&mut self) {
        self.arena_watermark = 0;
        self.completed_steps += 1;
        self.state = ReactorState::Idle;
    }
}

fn validate_entry_transition(state: ReactorState, entry: EntryKind) -> Result<(), Diagnostic> {
    if state != ReactorState::Idle {
        return Err(Diagnostic {
            code: DiagnosticCode::HybE3903,
            detail: "cannot enter cell while frame active or after one-shot exit",
        });
    }
    match entry {
        EntryKind::ReactorStep => Ok(()),
        EntryKind::OneShotStart | EntryKind::ReentrantHostCallback => Err(Diagnostic {
            code: DiagnosticCode::HybE3903,
            detail: "one-shot _start or re-entrant callback forbidden in reactor cell",
        }),
    }
}

pub fn snapshot_clone(value: &WireType) -> WireType {
    match value {
        WireType::Scalar(v) => WireType::Scalar(*v),
        WireType::ByteVec(bytes) => WireType::ByteVec(bytes.clone()),
        WireType::OwnedTree(nodes) => {
            WireType::OwnedTree(nodes.iter().map(snapshot_clone).collect())
        }
        other => other.clone(),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LocalBorrowRegion {
    pub active_shared_loans: usize,
    pub active_exclusive_loan: bool,
}

impl LocalBorrowRegion {
    pub fn new() -> Self {
        Self {
            active_shared_loans: 0,
            active_exclusive_loan: false,
        }
    }

    pub fn acquire_may_share(&mut self, exclusive: bool) -> Result<(), Diagnostic> {
        if exclusive {
            if self.active_exclusive_loan || self.active_shared_loans > 0 {
                return Err(Diagnostic {
                    code: DiagnosticCode::IntraCellLoanConflict,
                    detail: "intra-cell exclusive borrow conflicts with active loan",
                });
            }
            self.active_exclusive_loan = true;
            return Ok(());
        }
        if self.active_exclusive_loan {
            return Err(Diagnostic {
                code: DiagnosticCode::IntraCellLoanConflict,
                detail: "intra-cell shared borrow conflicts with active exclusive loan",
            });
        }
        self.active_shared_loans += 1;
        Ok(())
    }
}

#[allow(dead_code)]
fn main() {
    let spec = ModuleSpec {
        memory: MemoryModel::PrivateNonSharedMemory,
        imports: vec![ImportDecl::CoordinatorSend, ImportDecl::CoordinatorRecv],
        exports_cell_step: true,
    };
    let mut cell = IsolatedCell::instantiate(&spec).expect("valid module");
    let msg = WireType::OwnedTree(vec![WireType::Scalar(42), WireType::ByteVec(vec![1, 2, 3])]);
    let reply = cell.invoke_entry(EntryKind::ReactorStep, &msg).expect("step ok");
    assert_eq!(msg, reply);
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid_spec() -> ModuleSpec {
        ModuleSpec {
            memory: MemoryModel::PrivateNonSharedMemory,
            imports: vec![ImportDecl::CoordinatorSend, ImportDecl::CoordinatorRecv],
            exports_cell_step: true,
        }
    }

    #[test]
    fn test_p1_snapshot_isolation_across_cells() {
        let mut cell_a = IsolatedCell::instantiate(&valid_spec()).unwrap();
        let mut cell_b = IsolatedCell::instantiate(&valid_spec()).unwrap();
        let mut original = WireType::OwnedTree(vec![
            WireType::Scalar(100),
            WireType::ByteVec(vec![10, 20, 30]),
        ]);
        let received = cell_b
            .invoke_entry(EntryKind::ReactorStep, &original)
            .unwrap();
        if let WireType::OwnedTree(ref mut children) = original {
            children[0] = WireType::Scalar(999);
        }
        cell_a.linear_memory[0] = 0xFF;
        assert_eq!(cell_b.linear_memory[0], 0x00);
        assert_eq!(
            received,
            WireType::OwnedTree(vec![
                WireType::Scalar(100),
                WireType::ByteVec(vec![10, 20, 30]),
            ])
        );
    }

    #[test]
    fn test_p2_wire_and_module_admission_errors() {
        let bad_types = vec![
            WireType::RawPtr(0xdead_beef),
            WireType::MutableSliceAlias(1),
            WireType::ClosureWithEnv("env"),
            WireType::ChannelEnd(7),
            WireType::ForeignHandle(9),
            WireType::CyclicGraph,
            WireType::DepthExceeded,
        ];
        for item in bad_types {
            let err = admit_wire_sendable(&item, 0).unwrap_err();
            assert_eq!(err.code, DiagnosticCode::HybE3901);
        }
        let mut deep = WireType::Scalar(1);
        for _ in 0..9 {
            deep = WireType::OwnedTree(vec![deep]);
        }
        assert_eq!(admit_wire_sendable(&deep, 0).unwrap_err().code, DiagnosticCode::HybE3901);

        let shared_spec = ModuleSpec {
            memory: MemoryModel::SharedLinearMemory,
            imports: vec![],
            exports_cell_step: true,
        };
        assert_eq!(admit_cell_module(&shared_spec).unwrap_err().code, DiagnosticCode::HybE3902);

        let wasi_spec = ModuleSpec {
            memory: MemoryModel::PrivateNonSharedMemory,
            imports: vec![ImportDecl::AmbientWasiImport("wasi_snapshot_preview1::fd_write")],
            exports_cell_step: true,
        };
        assert_eq!(admit_cell_module(&wasi_spec).unwrap_err().code, DiagnosticCode::HybE3902);
    }

    #[test]
    fn test_f1_falsifier_imported_host_memory_rejected() {
        let bypassed = ModuleSpec {
            memory: MemoryModel::ImportedHostMemory,
            imports: vec![],
            exports_cell_step: true,
        };
        let err = IsolatedCell::instantiate(&bypassed).unwrap_err();
        assert_eq!(err.code, DiagnosticCode::HybE3902);
    }

    #[test]
    fn test_p3_persistent_reactor_without_reinstantiation() {
        let mut cell = IsolatedCell::instantiate(&valid_spec()).unwrap();
        for step in 0..1_000 {
            let msg = WireType::Scalar(step);
            let out = cell.invoke_entry(EntryKind::ReactorStep, &msg).unwrap();
            assert_eq!(out, WireType::Scalar(step));
            assert_eq!(cell.arena_watermark, 0);
        }
        assert_eq!(cell.completed_steps, 1_000);
    }

    #[test]
    fn test_f2_falsifier_reentrant_and_oneshot_rejected() {
        let mut cell = IsolatedCell::instantiate(&valid_spec()).unwrap();
        let msg = WireType::Scalar(1);
        let err_oneshot = cell.invoke_entry(EntryKind::OneShotStart, &msg).unwrap_err();
        assert_eq!(err_oneshot.code, DiagnosticCode::HybE3903);

        let _frame = cell.begin_step_frame(&msg).unwrap();
        let err_reenter = cell.invoke_entry(EntryKind::ReactorStep, &msg).unwrap_err();
        assert_eq!(err_reenter.code, DiagnosticCode::HybE3903);
        cell.finish_step_frame();
    }

    #[test]
    fn test_intra_cell_may_share_vs_cross_cell_wire() {
        let mut local_region = LocalBorrowRegion::new();
        assert!(local_region.acquire_may_share(false).is_ok());
        assert!(local_region.acquire_may_share(false).is_ok());
        let conflict = local_region.acquire_may_share(true).unwrap_err();
        assert_eq!(conflict.code, DiagnosticCode::IntraCellLoanConflict);

        let cross_cell_alias = WireType::MutableSliceAlias(0x1000);
        let wire_err = admit_wire_sendable(&cross_cell_alias, 0).unwrap_err();
        assert_eq!(wire_err.code, DiagnosticCode::HybE3901);
    }
}
