//! Spike 12 (`W-017`, #200): Operational Take-and-Detach Lowering Model.
//!
//! Dependency-free model of operational `move` lowering in `hy-emit` plus minimal
//! static gates for pinned/borrowed/const storage.
//! Run with: `rustc --edition=2021 --test spikes/spike12-operational-take-and-detach/take_detach_model.rs`
//! Invariants: moved-ness is an explicit binding flag, never inferred from storage
//! (nil is a legal move source); `HeapId(0)` and handle `0` are nil/invalid sentinels,
//! never allocation identities.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::atomic::{AtomicU64, Ordering};
use CheckError as E;
use StorageKind as S;

static NEXT_HEAP_ID: AtomicU64 = AtomicU64::new(1);
type Name = &'static str;
type Idents = BTreeSet<Ident>;

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct HeapId(pub u64);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub enum Ident {
    Heap(HeapId),
    Handle(u64),
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub enum StorageKind {
    #[default]
    ValueCopy,
    GoMap(HeapId), // HeapId(0) = nil map
    GoSlice { array: HeapId, cap_shared: bool },
    RustOwnHandle(u64), // 0 = invalid sentinel in hyrt HandleTable
    Composite(BTreeMap<Name, StorageKind>),
    PinnedForeign(Name), // e.g. sync.Mutex / atomic.Int64 (HYB-E3001)
    CyclicGraph(HeapId), // pointer cycle requiring fail-closed HYB-E3005
}

#[derive(Clone, Debug, Default, Eq, PartialEq)]
pub struct Binding {
    pub storage: StorageKind,
    pub is_const: bool,
    pub non_owned_origin: Option<Name>,
    pub live_borrow: bool,
    pub moved: bool,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LoweredMoveOp {
    ZeroAllocTake,          // `__hy_take(&x)` (0 allocs, O(1))
    ClippedSliceTake,       // `__hy_clip_slice(__hy_take(&s))` (0 allocs, O(1))
    SelectiveHeaderDetach,  // `__hy_detach_<T>(__hy_take(&x))`
    RustHandleSentinelTake, // `__hy_take(&h)` -> sets Go uint64 handle to 0
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum CheckError {
    E3001NotSendablePinned(Name),
    E3003UseAfterMove(Name),
    E3004NonOwnedMove(Name),
    E3005UnprovenAtBoundary(Name),
    E3006ConstImmutability(Name),
    E3101LoanConflict(Name),
}

#[derive(Clone, Debug, Default)]
pub struct TaskEnv {
    pub locals: BTreeMap<Name, Binding>,
}

pub fn fresh_heap() -> HeapId {
    HeapId(NEXT_HEAP_ID.fetch_add(1, Ordering::Relaxed))
}

impl StorageKind {
    fn leaves(&self) -> Vec<&S> {
        match self {
            S::Composite(m) => m.values().flat_map(S::leaves).collect(),
            leaf => vec![leaf],
        }
    }

    /// Non-nil heap and live handle identities reachable from this value.
    pub fn idents(&self) -> Idents {
        let ident = |l: &&S| match l {
            S::GoMap(id) | S::CyclicGraph(id) | S::GoSlice { array: id, .. } if id.0 != 0 => {
                Some(Ident::Heap(*id))
            }
            S::RustOwnHandle(h) if *h != 0 => Some(Ident::Handle(*h)),
            _ => None,
        };
        self.leaves().iter().filter_map(ident).collect()
    }

    pub fn check_transferable(&self, name: Name) -> Result<(), CheckError> {
        self.leaves().into_iter().try_for_each(|l| match l {
            S::PinnedForeign(ty) => Err(E::E3001NotSendablePinned(ty)),
            S::CyclicGraph(_) => Err(E::E3005UnprovenAtBoundary(name)),
            _ => Ok(()),
        })
    }

    /// Rebuilds the value with remapped headers/handles; every slice comes out clipped.
    fn rebuild(&self, heap: &mut dyn FnMut(HeapId) -> HeapId, handle: Option<u64>) -> S {
        match self {
            S::Composite(m) => {
                S::Composite(m.iter().map(|(k, v)| (*k, v.rebuild(heap, handle))).collect())
            }
            S::GoMap(id) => S::GoMap(heap(*id)),
            S::GoSlice { array, .. } => S::GoSlice { array: heap(*array), cap_shared: false },
            S::RustOwnHandle(h) => S::RustOwnHandle(handle.unwrap_or(*h)),
            other => other.clone(),
        }
    }

    /// `__hy_take` source state: every reference leaf becomes nil / sentinel 0.
    pub fn zeroed(&self) -> S {
        self.rebuild(&mut |_| HeapId(0), Some(0))
    }

    /// Receiver state: fresh headers for shared heaps, handles kept.
    pub fn detach_shared(&self, shared: &Idents) -> S {
        let mut fresh = |id| match shared.contains(&Ident::Heap(id)) {
            true => fresh_heap(),
            false => id,
        };
        self.rebuild(&mut fresh, None)
    }
}

impl TaskEnv {
    pub fn bind(&mut self, name: Name, storage: S) {
        self.locals.insert(name, Binding { storage, ..Binding::default() });
    }

    fn live(&self, name: Name) -> Result<&Binding, CheckError> {
        self.locals.get(name).filter(|b| !b.moved).ok_or(E::E3003UseAfterMove(name))
    }

    fn idents_except(&self, skip: Option<Name>) -> Idents {
        let peers = self.locals.iter().filter(|(n, _)| Some(**n) != skip);
        peers.flat_map(|(_, b)| b.storage.idents()).collect()
    }

    /// Operational `move <name>` across a `task` or `chan <- move` boundary.
    pub fn operational_move(&mut self, name: Name, rx: &mut TaskEnv, dst: Name) -> Result<LoweredMoveOp, E> {
        let b = self.live(name)?.clone();
        if b.is_const {
            return Err(E::E3006ConstImmutability(name));
        }
        if let Some(origin) = b.non_owned_origin {
            return Err(E::E3004NonOwnedMove(origin));
        }
        if b.live_borrow {
            return Err(E::E3101LoanConflict(name));
        }
        b.storage.check_transferable(name)?;
        // Identities still reachable from OTHER bindings (moved ones hold only nil/0).
        let peers = self.idents_except(Some(name));
        let shared: Idents = b.storage.idents().intersection(&peers).copied().collect();
        if rx.locals.contains_key(dst) || shared.iter().any(|i| matches!(i, Ident::Handle(_))) {
            return Err(E::E3005UnprovenAtBoundary(name)); // a Rust resource cannot be detached
        }
        let transferred = b.storage.detach_shared(&shared);
        let op = match &b.storage {
            S::RustOwnHandle(_) => LoweredMoveOp::RustHandleSentinelTake,
            _ if !shared.is_empty() => LoweredMoveOp::SelectiveHeaderDetach,
            s if *s != transferred => LoweredMoveOp::ClippedSliceTake,
            _ => LoweredMoveOp::ZeroAllocTake,
        };
        let src = self.locals.get_mut(name).unwrap(); // operational take: zero the source
        (src.storage, src.moved) = (b.storage.zeroed(), true);
        rx.bind(dst, transferred);
        Ok(op)
    }

    pub fn mutate(&self, name: Name) -> Result<(), CheckError> {
        let b = self.live(name)?;
        b.is_const.then_some(()).map_or(Ok(()), |_| Err(E::E3006ConstImmutability(name)))
    }

    pub fn disjoint_from(&self, other: &TaskEnv) -> bool {
        self.idents_except(None).is_disjoint(&other.idents_except(None))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn composite(fields: &[(&'static str, StorageKind)]) -> StorageKind {
        StorageKind::Composite(fields.iter().cloned().collect())
    }

    #[test]
    fn fast_path_unaliased_move_is_zero_alloc_take_and_zeroes_source() {
        let mut main_env = TaskEnv::default();
        let mut worker_env = TaskEnv::default();
        let h = fresh_heap();
        main_env.bind("m", StorageKind::GoMap(h));

        let op = main_env
            .operational_move("m", &mut worker_env, "w")
            .unwrap();
        assert_eq!(op, LoweredMoveOp::ZeroAllocTake);
        assert_eq!(
            main_env.mutate("m"),
            Err(CheckError::E3003UseAfterMove("m"))
        );
        assert_eq!(worker_env.mutate("w"), Ok(()));
        assert!(main_env.disjoint_from(&worker_env));
    }

    #[test]
    fn shared_peer_move_detaches_header_so_missed_alias_cannot_race() {
        // Models report chains #38/#105/#120 and #75/#137/#206: `m` and `h.m`
        // share `HeapId`, and `h` is moved to `worker` while `main` mutates `m`.
        let mut main_env = TaskEnv::default();
        let mut worker_env = TaskEnv::default();
        let h = fresh_heap();
        main_env.bind("m", StorageKind::GoMap(h));
        main_env.bind("holder", composite(&[("m", StorageKind::GoMap(h))]));

        let op = main_env
            .operational_move("holder", &mut worker_env, "w_holder")
            .unwrap();
        assert_eq!(op, LoweredMoveOp::SelectiveHeaderDetach);
        assert!(main_env.disjoint_from(&worker_env));
    }

    #[test]
    fn rust_own_handle_move_zeroes_source_to_hyrt_invalid_sentinel() {
        let mut main_env = TaskEnv::default();
        let mut worker_env = TaskEnv::default();
        main_env.bind("re", StorageKind::RustOwnHandle(0x0001_0000_002a));

        let op = main_env
            .operational_move("re", &mut worker_env, "w_re")
            .unwrap();
        assert_eq!(op, LoweredMoveOp::RustHandleSentinelTake);
        assert_eq!(main_env.locals["re"].storage, StorageKind::RustOwnHandle(0));
        assert_eq!(
            worker_env.locals["w_re"].storage,
            StorageKind::RustOwnHandle(0x0001_0000_002a)
        );
        assert_eq!(
            main_env.mutate("re"),
            Err(CheckError::E3003UseAfterMove("re"))
        );
    }

    #[test]
    fn executable_30_case_corpus_and_p0_matrix() {
        // 30 distinct shapes across tests/corpus (01..50) and historical P0
        // soundness chains (#38, #75, #98, #105, #120, #137, #206, #212, #309, #354).
        for case_id in 1..=30 {
            let mut caller = TaskEnv::default();
            let mut callee = TaskEnv::default();
            let h1 = fresh_heap();
            let h2 = fresh_heap();
            match case_id {
                // 1..=10: 10 distinct un-aliased owned shapes -> ZeroAllocTake
                1 => {
                    caller.bind("v", S::ValueCopy);
                    assert_eq!(caller.operational_move("v", &mut callee, "y"), Ok(LoweredMoveOp::ZeroAllocTake));
                }
                2 => {
                    caller.bind("nil_m", S::GoMap(HeapId(0)));
                    assert_eq!(caller.operational_move("nil_m", &mut callee, "y"), Ok(LoweredMoveOp::ZeroAllocTake));
                }
                3 => {
                    caller.bind("m", S::GoMap(h1));
                    assert_eq!(caller.operational_move("m", &mut callee, "y"), Ok(LoweredMoveOp::ZeroAllocTake));
                    assert!(caller.disjoint_from(&callee));
                }
                4 => {
                    caller.bind("s", S::GoSlice { array: h1, cap_shared: false });
                    assert_eq!(caller.operational_move("s", &mut callee, "y"), Ok(LoweredMoveOp::ZeroAllocTake));
                    assert!(caller.disjoint_from(&callee));
                }
                5 => {
                    caller.bind("job", composite(&[("id", S::ValueCopy), ("payload", S::GoMap(h1))]));
                    assert_eq!(caller.operational_move("job", &mut callee, "y"), Ok(LoweredMoveOp::ZeroAllocTake));
                    assert!(caller.disjoint_from(&callee));
                }
                6 => {
                    caller.bind(
                        "pair",
                        composite(&[
                            ("m", S::GoMap(h1)),
                            ("s", S::GoSlice { array: h2, cap_shared: false }),
                        ]),
                    );
                    assert_eq!(caller.operational_move("pair", &mut callee, "y"), Ok(LoweredMoveOp::ZeroAllocTake));
                    assert!(caller.disjoint_from(&callee));
                }
                7 => {
                    caller.bind("nested", composite(&[("inner", composite(&[("m", S::GoMap(h1))]))]));
                    assert_eq!(caller.operational_move("nested", &mut callee, "y"), Ok(LoweredMoveOp::ZeroAllocTake));
                    assert!(caller.disjoint_from(&callee));
                }
                8 => {
                    // Move after an earlier peer binding `prev` was already moved (prev holds HeapId(0)).
                    let mut w0 = TaskEnv::default();
                    caller.bind("prev", S::GoMap(h1));
                    caller.operational_move("prev", &mut w0, "w0").unwrap();
                    caller.bind("cur", S::GoMap(h2));
                    assert_eq!(caller.operational_move("cur", &mut callee, "y"), Ok(LoweredMoveOp::ZeroAllocTake));
                    assert!(caller.disjoint_from(&callee));
                }
                9 => {
                    // Sequential hop `caller -> mid -> callee` with no surviving alias.
                    let mut mid = TaskEnv::default();
                    caller.bind("x", S::GoMap(h1));
                    assert_eq!(caller.operational_move("x", &mut mid, "m"), Ok(LoweredMoveOp::ZeroAllocTake));
                    assert_eq!(mid.operational_move("m", &mut callee, "y"), Ok(LoweredMoveOp::ZeroAllocTake));
                    assert!(caller.disjoint_from(&callee) && mid.disjoint_from(&callee));
                }
                10 => {
                    // Composite holding both a GoMap and a RustOwnHandle.
                    caller.bind("res", composite(&[("m", S::GoMap(h1)), ("h", S::RustOwnHandle(77))]));
                    assert_eq!(caller.operational_move("res", &mut callee, "y"), Ok(LoweredMoveOp::ZeroAllocTake));
                    assert!(caller.disjoint_from(&callee));
                }
                // 11..=18: 8 distinct shared-peer / P0 report-chain shapes -> SelectiveHeaderDetach
                11 => {
                    // Direct peer map alias (#38 base).
                    caller.bind("orig", S::GoMap(h1));
                    caller.bind("alias", S::GoMap(h1));
                    assert_eq!(caller.operational_move("alias", &mut callee, "y"), Ok(LoweredMoveOp::SelectiveHeaderDetach));
                    assert!(caller.disjoint_from(&callee));
                }
                12 => {
                    // Call/method return wrapping caller's map (#38 / #105 / #120).
                    caller.bind("orig", S::GoMap(h1));
                    caller.bind("wrap", composite(&[("m", S::GoMap(h1))]));
                    assert_eq!(caller.operational_move("wrap", &mut callee, "y"), Ok(LoweredMoveOp::SelectiveHeaderDetach));
                    assert!(caller.disjoint_from(&callee));
                }
                13 => {
                    // Shared slice backing array between peer and struct (#98 / #138).
                    caller.bind("buf", S::GoSlice { array: h1, cap_shared: false });
                    caller.bind("wrap", composite(&[("s", S::GoSlice { array: h1, cap_shared: true })]));
                    assert_eq!(caller.operational_move("wrap", &mut callee, "y"), Ok(LoweredMoveOp::SelectiveHeaderDetach));
                    assert!(caller.disjoint_from(&callee));
                }
                14 => {
                    // Partial sharing: `shared_m` shares h1 with `orig`, `unique_m` owns h2.
                    caller.bind("orig", S::GoMap(h1));
                    caller.bind("partial", composite(&[("shared_m", S::GoMap(h1)), ("unique_m", S::GoMap(h2))]));
                    assert_eq!(caller.operational_move("partial", &mut callee, "y"), Ok(LoweredMoveOp::SelectiveHeaderDetach));
                    assert!(caller.disjoint_from(&callee));
                    if let S::Composite(fields) = &callee.locals["y"].storage {
                        assert_ne!(fields["shared_m"], S::GoMap(h1));
                        assert_eq!(fields["unique_m"], S::GoMap(h2));
                    } else {
                        panic!("expected composite");
                    }
                }
                15 => {
                    // Nested 2-level composite sharing inner map (#137 / #206).
                    caller.bind("orig", S::GoMap(h1));
                    caller.bind("outer", composite(&[("inner", composite(&[("m", S::GoMap(h1))]))]));
                    assert_eq!(caller.operational_move("outer", &mut callee, "y"), Ok(LoweredMoveOp::SelectiveHeaderDetach));
                    assert!(caller.disjoint_from(&callee));
                }
                16 => {
                    // Cross-field sibling alias inside peer struct (#212).
                    caller.bind("peer", composite(&[("a", S::GoMap(h1)), ("b", S::GoMap(h1))]));
                    caller.bind("moved_s", composite(&[("a", S::GoMap(h1))]));
                    assert_eq!(caller.operational_move("moved_s", &mut callee, "y"), Ok(LoweredMoveOp::SelectiveHeaderDetach));
                    assert!(caller.disjoint_from(&callee));
                }
                17 => {
                    // Rebind chain where `m` still holds `h1` after `p2 -> h` hop (#75 / #206).
                    let mut hop = TaskEnv::default();
                    caller.bind("m", S::GoMap(h1));
                    caller.bind("p2", composite(&[("m", S::GoMap(h1))]));
                    assert_eq!(caller.operational_move("p2", &mut hop, "h"), Ok(LoweredMoveOp::SelectiveHeaderDetach));
                    assert!(caller.disjoint_from(&hop));
                    assert_eq!(hop.operational_move("h", &mut callee, "y"), Ok(LoweredMoveOp::ZeroAllocTake));
                    assert!(caller.disjoint_from(&callee));
                }
                18 => {
                    // Simultaneous shared map and shared slice backing array.
                    caller.bind("m", S::GoMap(h1));
                    caller.bind("s", S::GoSlice { array: h2, cap_shared: false });
                    caller.bind(
                        "both",
                        composite(&[
                            ("m", S::GoMap(h1)),
                            ("s", S::GoSlice { array: h2, cap_shared: true }),
                        ]),
                    );
                    assert_eq!(caller.operational_move("both", &mut callee, "y"), Ok(LoweredMoveOp::SelectiveHeaderDetach));
                    assert!(caller.disjoint_from(&callee));
                }
                // 19..=22: 4 distinct sub-slice capacity-clipping shapes -> ClippedSliceTake
                19 => {
                    caller.bind("sub", S::GoSlice { array: h1, cap_shared: true });
                    assert_eq!(caller.operational_move("sub", &mut callee, "y"), Ok(LoweredMoveOp::ClippedSliceTake));
                }
                20 => {
                    caller.bind("holder", composite(&[("sub", S::GoSlice { array: h1, cap_shared: true })]));
                    assert_eq!(caller.operational_move("holder", &mut callee, "y"), Ok(LoweredMoveOp::ClippedSliceTake));
                }
                21 => {
                    caller.bind(
                        "deep",
                        composite(&[("inner", composite(&[("sub", S::GoSlice { array: h1, cap_shared: true })]))]),
                    );
                    assert_eq!(caller.operational_move("deep", &mut callee, "y"), Ok(LoweredMoveOp::ClippedSliceTake));
                }
                22 => {
                    caller.bind(
                        "mixed",
                        composite(&[
                            ("m", S::GoMap(h1)),
                            ("sub", S::GoSlice { array: h2, cap_shared: true }),
                        ]),
                    );
                    assert_eq!(caller.operational_move("mixed", &mut callee, "y"), Ok(LoweredMoveOp::ClippedSliceTake));
                }
                // 23..=25: 3 CGo/Rust staticlib bridge own-handle shapes -> RustHandleSentinelTake
                23 => {
                    // 05-json-transform serde_json document own-handle
                    caller.bind("doc", S::RustOwnHandle(0x0001_0000_0005));
                    assert_eq!(caller.operational_move("doc", &mut callee, "y"), Ok(LoweredMoveOp::RustHandleSentinelTake));
                    assert!(caller.disjoint_from(&callee));
                }
                24 => {
                    // 09-go-imports-hybrid regex own-handle
                    caller.bind("re", S::RustOwnHandle(0x0001_0000_0009));
                    assert_eq!(caller.operational_move("re", &mut callee, "y"), Ok(LoweredMoveOp::RustHandleSentinelTake));
                    assert!(caller.disjoint_from(&callee));
                }
                25 => {
                    // 10-rust-crate-import hasher own-handle
                    caller.bind("hasher", S::RustOwnHandle(0x0002_0000_000a));
                    assert_eq!(caller.operational_move("hasher", &mut callee, "y"), Ok(LoweredMoveOp::RustHandleSentinelTake));
                    assert!(caller.disjoint_from(&callee));
                }
                // 26..=30: 5 static fail-closed gates matching compiler/crates/hy-diag/data/diagnostics.json
                26 => {
                    caller.bind("mu", composite(&[("lock", S::PinnedForeign("sync.Mutex"))]));
                    assert_eq!(
                        caller.operational_move("mu", &mut callee, "y"),
                        Err(CheckError::E3001NotSendablePinned("sync.Mutex"))
                    );
                }
                27 => {
                    caller.bind("cyc", S::CyclicGraph(h1));
                    assert_eq!(
                        caller.operational_move("cyc", &mut callee, "y"),
                        Err(CheckError::E3005UnprovenAtBoundary("cyc"))
                    );
                }
                28 => {
                    caller.bind("p", S::GoMap(h1));
                    caller.locals.get_mut("p").unwrap().non_owned_origin = Some("param p");
                    assert_eq!(
                        caller.operational_move("p", &mut callee, "y"),
                        Err(CheckError::E3004NonOwnedMove("param p"))
                    );
                }
                29 => {
                    // #309: moving or mutating a `const` binding is rejected with HYB-E3006 (T22).
                    caller.bind("k", S::GoMap(h1));
                    caller.locals.get_mut("k").unwrap().is_const = true;
                    assert_eq!(
                        caller.operational_move("k", &mut callee, "y"),
                        Err(CheckError::E3006ConstImmutability("k"))
                    );
                    assert_eq!(caller.mutate("k"), Err(CheckError::E3006ConstImmutability("k")));
                }
                30 => {
                    caller.bind("m", S::GoMap(h1));
                    caller.locals.get_mut("m").unwrap().live_borrow = true;
                    assert_eq!(
                        caller.operational_move("m", &mut callee, "y"),
                        Err(CheckError::E3101LoanConflict("m"))
                    );
                }
                _ => unreachable!(),
            }
        }
    }

    // R1: moved state is an explicit flag so a struct with a value-typed field
    // cannot be moved twice.
    #[test]
    fn r1_double_move_of_mixed_struct_is_use_after_move() {
        let (mut c, mut w1, mut w2) = (TaskEnv::default(), TaskEnv::default(), TaskEnv::default());
        c.bind("s", composite(&[("n", S::ValueCopy), ("m", S::GoMap(fresh_heap()))]));
        assert!(c.operational_move("s", &mut w1, "a").is_ok());
        assert_eq!(
            c.operational_move("s", &mut w2, "b"),
            Err(CheckError::E3003UseAfterMove("s"))
        );
    }

    // R2: HeapId(0) sentinel is never counted as a shared heap identity.
    #[test]
    fn r2_zero_sentinel_is_never_a_shared_heap() {
        let (mut c, mut w1, mut w2, mut w3) = (
            TaskEnv::default(),
            TaskEnv::default(),
            TaskEnv::default(),
            TaskEnv::default(),
        );
        c.bind("a", S::GoMap(fresh_heap()));
        c.operational_move("a", &mut w1, "a").unwrap();
        c.bind("s", composite(&[("n", S::ValueCopy), ("m", S::GoMap(fresh_heap()))]));
        c.operational_move("s", &mut w2, "s").unwrap();
        assert_eq!(
            c.operational_move("s", &mut w3, "s"),
            Err(CheckError::E3003UseAfterMove("s"))
        );
    }

    // R3: a plain ValueCopy binding cannot be moved twice.
    #[test]
    fn r3_value_copy_double_move_rejected() {
        let (mut c, mut w) = (TaskEnv::default(), TaskEnv::default());
        c.bind("v", S::ValueCopy);
        c.operational_move("v", &mut w, "a").unwrap();
        assert_eq!(
            c.operational_move("v", &mut w, "b"),
            Err(CheckError::E3003UseAfterMove("v"))
        );
    }

    // R4: nil map is a legal move source and does not falsely trigger E3003.
    #[test]
    fn r4_nil_map_move_is_not_use_after_move() {
        let (mut c, mut w) = (TaskEnv::default(), TaskEnv::default());
        c.bind("m", S::GoMap(HeapId(0)));
        assert!(c.operational_move("m", &mut w, "y").is_ok());
    }

    // R5: capacity clipping applies recursively to nested struct slice fields.
    #[test]
    fn r5_nested_cap_shared_slice_is_clipped() {
        let (mut c, mut w) = (TaskEnv::default(), TaskEnv::default());
        let h = fresh_heap();
        c.bind("h", composite(&[("s", S::GoSlice { array: h, cap_shared: true })]));
        assert_eq!(
            c.operational_move("h", &mut w, "y"),
            Ok(LoweredMoveOp::ClippedSliceTake)
        );
        assert_eq!(
            w.locals["y"].storage,
            composite(&[("s", S::GoSlice { array: h, cap_shared: false })])
        );
    }

    // R6: duplicated own handles are rejected fail-closed (HYB-E3005) because
    // a Rust resource behind hyrt cannot be cloned by a Go-side header detach.
    #[test]
    fn r6_duplicated_own_handle_is_rejected_fail_closed() {
        let (mut c, mut w) = (TaskEnv::default(), TaskEnv::default());
        c.bind("a", S::RustOwnHandle(42));
        c.bind("b", S::RustOwnHandle(42));
        assert_eq!(
            c.operational_move("a", &mut w, "y"),
            Err(CheckError::E3005UnprovenAtBoundary("a"))
        );
    }

    // R7: binding into an occupied receiver slot is rejected so an existing
    // handle in the destination cannot be silently overwritten.
    #[test]
    fn r7_receiver_slot_collision_rejected() {
        let (mut c, mut w) = (TaskEnv::default(), TaskEnv::default());
        w.bind("y", S::RustOwnHandle(7));
        c.bind("h", S::RustOwnHandle(9));
        assert_eq!(
            c.operational_move("h", &mut w, "y"),
            Err(CheckError::E3005UnprovenAtBoundary("h"))
        );
        assert_eq!(w.locals["y"].storage, S::RustOwnHandle(7));
    }

    // R8: when `may_share` misses an external alias (e.g. `defer f(m)` evaluated
    // before `move m`), the local env sees no peer and picks the fast path —
    // proving why `may_share` / defer-loan accounting remains load-bearing.
    #[test]
    fn r8_missed_external_alias_documents_why_may_share_is_load_bearing() {
        let (mut c, mut w) = (TaskEnv::default(), TaskEnv::default());
        let h = fresh_heap();
        c.bind("m", S::GoMap(h));
        let external_defer_arg = S::GoMap(h);
        assert_eq!(
            c.operational_move("m", &mut w, "y"),
            Ok(LoweredMoveOp::ZeroAllocTake)
        );
        assert!(!external_defer_arg.idents().is_disjoint(&w.locals["y"].storage.idents()));
    }
}
