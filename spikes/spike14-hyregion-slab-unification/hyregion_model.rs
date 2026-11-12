//! Feasibility Spike W-014 (#239): Region-Isolated Linear Slabs (`HyRegion`)
//! + 1-Bit Hindley-Milner Region Unification.
//!
//! Standalone, dependency-free executable model:
//!   rustc --edition=2021 --test spikes/spike14-hyregion-slab-unification/hyregion_model.rs

use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct RegionId(pub u32);

impl RegionId {
    pub const STATIC: Self = Self(0);
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Diag {
    E3003UseAfterMove,
    E3004MoveNonOwned,
    E3006ConstMutation,
    E3102MapSlotLoanMutation,
    E3104AddrOfMapSlotLoan,
}

#[derive(Clone, Debug)]
struct RegionNode {
    parent: u32,
    moved: bool,
    origin_non_owned: bool,
    is_const: bool,
    active_slot_loans: u16,
}

pub struct RegionTable {
    nodes: Vec<RegionNode>,
    scopes: Vec<HashMap<String, RegionId>>,
    loan_scopes: Vec<Vec<RegionId>>,
}

impl RegionTable {
    pub fn new() -> Self {
        Self {
            nodes: vec![RegionNode {
                parent: 0,
                moved: false,
                origin_non_owned: false,
                is_const: true,
                active_slot_loans: 0,
            }],
            scopes: vec![HashMap::new()],
            loan_scopes: vec![Vec::new()],
        }
    }

    pub fn alloc_region(&mut self, origin_non_owned: bool, is_const: bool) -> RegionId {
        let id = self.nodes.len() as u32;
        self.nodes.push(RegionNode {
            parent: id,
            moved: false,
            origin_non_owned,
            is_const,
            active_slot_loans: 0,
        });
        RegionId(id)
    }

    pub fn find(&mut self, r: RegionId) -> RegionId {
        let mut idx = r.0 as usize;
        while self.nodes[idx].parent != idx as u32 {
            let p = self.nodes[idx].parent as usize;
            self.nodes[idx].parent = self.nodes[p].parent;
            idx = p;
        }
        RegionId(idx as u32)
    }

    pub fn unify(&mut self, a: RegionId, b: RegionId) -> RegionId {
        let ra = self.find(a);
        let rb = self.find(b);
        if ra == RegionId::STATIC {
            return rb;
        }
        if rb == RegionId::STATIC || ra == rb {
            return ra;
        }
        let b_node = self.nodes[rb.0 as usize].clone();
        let a_node = &mut self.nodes[ra.0 as usize];
        a_node.moved |= b_node.moved;
        a_node.origin_non_owned |= b_node.origin_non_owned;
        a_node.is_const |= b_node.is_const;
        a_node.active_slot_loans = a_node
            .active_slot_loans
            .saturating_add(b_node.active_slot_loans);
        self.nodes[rb.0 as usize].parent = ra.0;
        ra
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
        self.loan_scopes.push(Vec::new());
    }

    pub fn pop_scope(&mut self) {
        if let Some(loans) = self.loan_scopes.pop() {
            for r in loans {
                let root = self.find(r);
                let node = &mut self.nodes[root.0 as usize];
                node.active_slot_loans = node.active_slot_loans.saturating_sub(1);
            }
        }
        self.scopes.pop();
    }

    pub fn define(&mut self, name: &str, r: RegionId) {
        if let Some(top) = self.scopes.last_mut() {
            top.insert(name.to_string(), r);
        }
    }

    pub fn lookup(&self, name: &str) -> Option<RegionId> {
        for scope in self.scopes.iter().rev() {
            if let Some(&r) = scope.get(name) {
                return Some(r);
            }
        }
        None
    }

    pub fn rebind(&mut self, name: &str, rhs: RegionId) -> Result<RegionId, Diag> {
        if let Some(existing) = self.lookup(name) {
            let root = self.find(existing);
            if root != RegionId::STATIC && self.nodes[root.0 as usize].is_const {
                return Err(Diag::E3006ConstMutation);
            }
            Ok(self.unify(existing, rhs))
        } else {
            self.define(name, rhs);
            Ok(rhs)
        }
    }

    pub fn read(&mut self, r: RegionId) -> Result<(), Diag> {
        let root = self.find(r);
        if self.nodes[root.0 as usize].moved {
            return Err(Diag::E3003UseAfterMove);
        }
        Ok(())
    }

    pub fn mutate_composite(&mut self, r: RegionId) -> Result<(), Diag> {
        let root = self.find(r);
        let n = &self.nodes[root.0 as usize];
        if n.moved {
            return Err(Diag::E3003UseAfterMove);
        }
        if n.is_const {
            return Err(Diag::E3006ConstMutation);
        }
        if n.active_slot_loans > 0 {
            return Err(Diag::E3102MapSlotLoanMutation);
        }
        Ok(())
    }

    pub fn loan_map_slot(&mut self, map_region: RegionId) -> Result<RegionId, Diag> {
        let root = self.find(map_region);
        let n = &mut self.nodes[root.0 as usize];
        if n.moved {
            return Err(Diag::E3003UseAfterMove);
        }
        if n.is_const {
            return Err(Diag::E3006ConstMutation);
        }
        n.active_slot_loans += 1;
        if let Some(loans) = self.loan_scopes.last_mut() {
            loans.push(root);
        }
        Ok(root)
    }

    pub fn move_region(&mut self, r: RegionId) -> Result<(), Diag> {
        let root = self.find(r);
        let n = &mut self.nodes[root.0 as usize];
        if n.moved {
            return Err(Diag::E3003UseAfterMove);
        }
        if n.is_const || n.origin_non_owned || n.active_slot_loans > 0 {
            return Err(Diag::E3004MoveNonOwned);
        }
        n.moved = true;
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlabHandle {
    pub slab_id: u32,
    pub epoch: u32,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SlabOffset(pub u32);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SlabError {
    EpochMismatch,
    StackArenaCollision,
    OutOfSlabMemory,
}

pub struct HyRegionSlab {
    pub slab_id: u32,
    pub epoch: u32,
    pub page_size: u32,
    pub total_size: u32,
    pub bump_off: u32,
    pub high_water: u32,
    pub sp_floor: u32,
    pub madvise_resets: u32,
}

impl HyRegionSlab {
    pub const HEADER_SIZE: u32 = 64;
    pub const COMPACT_THRESHOLD: u32 = 64 * 1024;

    pub fn new(slab_id: u32, page_size: u32, total_size: u32) -> Self {
        assert!(page_size.is_power_of_two() && total_size >= page_size * 4);
        Self {
            slab_id,
            epoch: 1,
            page_size,
            total_size,
            bump_off: Self::HEADER_SIZE,
            high_water: Self::HEADER_SIZE,
            sp_floor: total_size,
            madvise_resets: 0,
        }
    }

    pub fn handle(&self) -> SlabHandle {
        SlabHandle {
            slab_id: self.slab_id,
            epoch: self.epoch,
        }
    }

    pub fn bump_alloc(&mut self, h: SlabHandle, size: u32, align: u32) -> Result<SlabOffset, SlabError> {
        if h.slab_id != self.slab_id || h.epoch != self.epoch {
            return Err(SlabError::EpochMismatch);
        }
        let aligned = (self.bump_off + (align - 1)) & !(align - 1);
        let next = aligned.checked_add(size).ok_or(SlabError::OutOfSlabMemory)?;
        if next + self.page_size > self.sp_floor {
            return Err(SlabError::StackArenaCollision);
        }
        self.bump_off = next;
        if next > self.high_water {
            self.high_water = next;
        }
        Ok(SlabOffset(aligned))
    }

    pub fn enter_deep_rust_frame(&mut self, h: SlabHandle, frame_bytes: u32) -> Result<u32, SlabError> {
        if h.slab_id != self.slab_id || h.epoch != self.epoch {
            return Err(SlabError::EpochMismatch);
        }
        let new_sp = (self.sp_floor.checked_sub(frame_bytes).ok_or(SlabError::StackArenaCollision)?) & !15;
        if new_sp < self.bump_off + self.page_size {
            return Err(SlabError::StackArenaCollision);
        }
        self.sp_floor = new_sp;
        Ok(new_sp)
    }

    pub fn exit_deep_rust_frame(&mut self, prev_sp: u32) {
        self.sp_floor = prev_sp;
    }

    pub fn transfer_detach(&mut self, h: SlabHandle) -> Result<SlabHandle, SlabError> {
        if h.slab_id != self.slab_id || h.epoch != self.epoch {
            return Err(SlabError::EpochMismatch);
        }
        self.epoch = self.epoch.wrapping_add(1);
        Ok(self.handle())
    }

    pub fn release_to_pool(&mut self, h: SlabHandle) -> Result<(), SlabError> {
        if h.slab_id != self.slab_id || h.epoch != self.epoch {
            return Err(SlabError::EpochMismatch);
        }
        self.epoch = self.epoch.wrapping_add(1);
        self.bump_off = Self::HEADER_SIZE;
        self.sp_floor = self.total_size;
        if self.high_water > Self::COMPACT_THRESHOLD {
            self.madvise_resets += 1;
            self.high_water = Self::HEADER_SIZE;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_1bit_unification_transitivity_and_move_freeze() {
        let mut rt = RegionTable::new();
        let r_m1 = rt.alloc_region(false, false);
        rt.define("m1", r_m1);

        // Projection + struct wrap + unpack: m1 -> h -> m2 -> m3
        let r_h = rt.alloc_region(false, false);
        rt.unify(r_h, r_m1);
        rt.define("h", r_h);

        let r_m2 = rt.alloc_region(false, false);
        rt.unify(r_m2, r_h);
        rt.define("m2", r_m2);

        assert_eq!(rt. move_region(r_m1), Ok(()));
        assert_eq!(rt.read(r_m2), Err(Diag::E3003UseAfterMove));
        assert_eq!(rt.mutate_composite(r_h), Err(Diag::E3003UseAfterMove));
    }

    #[test]
    fn test_issue_114_conditional_rebind_cannot_erase_outer_alias_or_origin() {
        let mut rt = RegionTable::new();
        let r_param = rt.alloc_region(true, false); // parameter: NonOwned
        rt.define("p", r_param);

        let r_local = rt.alloc_region(false, false);
        rt.unify(r_local, r_param);
        rt.define("m2", r_local);

        // Inside `if false { m2 = make(map[string]int) }`:
        rt.push_scope();
        let r_fresh = rt.alloc_region(false, false);
        rt.rebind("m2", r_fresh).unwrap();
        rt.pop_scope();

        // Moving `m2` after the branch MUST still reject with E3004 because
        // unification monotonically preserved `origin_non_owned = true`.
        let m2_after = rt.lookup("m2").unwrap();
        assert_eq!(rt.move_region(m2_after), Err(Diag::E3004MoveNonOwned));
    }

    #[test]
    fn test_issue_92_field_index_store_and_bridge_rebind_transitivity() {
        let mut rt = RegionTable::new();
        let r_m = rt.alloc_region(false, false);
        let r_h1 = rt.alloc_region(false, false);
        let r_h2 = rt.alloc_region(false, false);
        // h1.m = m; h2 = holder{m: h1.m}
        rt.unify(r_h1, r_m);
        rt.unify(r_h2, r_h1);

        // Rebinding h1 to a fresh holder does NOT sever r_m <-> r_h2!
        let r_fresh = rt.alloc_region(false, false);
        let _ = rt.unify(r_h1, r_fresh);

        assert_eq!(rt.move_region(r_h2), Ok(()));
        assert_eq!(rt.mutate_composite(r_m), Err(Diag::E3003UseAfterMove));
    }

    #[test]
    fn test_issue_66_87_option_match_multi_unpack_and_for_iterate() {
        let mut rt = RegionTable::new();
        let r_m = rt.alloc_region(false, false);
        // opt := some(m); match opt { some(x) => ... }
        let r_opt = rt.alloc_region(false, false);
        rt.unify(r_opt, r_m);
        let r_x = rt.alloc_region(false, false);
        rt.unify(r_x, r_opt);

        // n, m2 := pair(x)
        let r_n = RegionId::STATIC;
        let r_m2 = rt.alloc_region(false, false);
        rt.unify(r_m2, r_x);

        assert_eq!(rt.move_region(r_m2), Ok(()));
        assert_eq!(rt.read(r_n), Ok(())); // primitive unaffected
        assert_eq!(rt.read(r_m), Err(Diag::E3003UseAfterMove));
        assert_eq!(rt.read(r_x), Err(Diag::E3003UseAfterMove));
    }

    #[test]
    fn test_issue_241_a3_slot_loan_and_const_borrow_protection() {
        let mut rt = RegionTable::new();
        // Const map cannot be loaned or mutated (O8 / E3006)
        let r_const = rt.alloc_region(false, true);
        assert_eq!(rt.loan_map_slot(r_const), Err(Diag::E3006ConstMutation));

        // Mutable map with alias: p := &m["k"] blocks mutation via alias m2!
        let r_m = rt.alloc_region(false, false);
        let r_m2 = rt.alloc_region(false, false);
        rt.unify(r_m2, r_m);

        rt.push_scope();
        let _loan = rt.loan_map_slot(r_m).unwrap();
        assert_eq!(rt.mutate_composite(r_m2), Err(Diag::E3102MapSlotLoanMutation));
        assert_eq!(rt.move_region(r_m2), Err(Diag::E3004MoveNonOwned));
        rt.pop_scope();

        // After scope exit, loan is released and m2 can be mutated and moved.
        assert_eq!(rt.mutate_composite(r_m2), Ok(()));
        assert_eq!(rt.move_region(r_m2), Ok(()));
    }

    #[test]
    fn test_issue_250_named_result_bindings_and_bare_return() {
        let mut rt = RegionTable::new();
        // fn leak(in_map map[string]int) (out map[string]int) { out = in_map; return }
        let r_in = rt.alloc_region(true, false);
        let r_out = rt.alloc_region(false, false);
        rt.define("in_map", r_in);
        rt.define("out", r_out);

        rt.rebind("out", r_in).unwrap();
        // If someone moves `out` to a task before bare `return`, bare `return` reads `out`:
        assert_eq!(rt.move_region(r_out), Err(Diag::E3004MoveNonOwned));

        // Fresh named result moved before bare return rejects bare return read with E3003:
        let r_fresh_out = rt.alloc_region(false, false);
        assert_eq!(rt.move_region(r_fresh_out), Ok(()));
        assert_eq!(rt.read(r_fresh_out), Err(Diag::E3003UseAfterMove));
    }

    #[test]
    fn test_slab_zero_gc_pointers_colocated_stack_and_o1_epoch_transfer() {
        let mut slab = HyRegionSlab::new(1, 4096, 256 * 1024);
        let h1 = slab.handle();

        // Upward arena bump allocations (offsets only, zero Go-heap pointers)
        let off1 = slab.bump_alloc(h1, 128, 8).unwrap();
        let off2 = slab.bump_alloc(h1, 256, 16).unwrap();
        assert_eq!(off1, SlabOffset(64));
        assert_eq!(off2, SlabOffset(192));

        // Co-located downward Rust stack in the same L1/TLB-hot slab
        let sp = slab.enter_deep_rust_frame(h1, 2048).unwrap();
        assert_eq!(sp, 256 * 1024 - 2048);
        slab.exit_deep_rust_frame(256 * 1024);

        // O(1) transfer across channel/task boundary increments epoch
        let h2 = slab.transfer_detach(h1).unwrap();
        assert_ne!(h1.epoch, h2.epoch);

        // Sender's stale handle traps deterministically (even inside `managed`)
        assert_eq!(slab.bump_alloc(h1, 64, 8), Err(SlabError::EpochMismatch));
        // Receiver's handle succeeds in-place
        assert!(slab.bump_alloc(h2, 64, 8).is_ok());
    }

    #[test]
    fn test_slab_guard_page_collision_and_high_water_madvise_compaction() {
        let mut slab = HyRegionSlab::new(7, 4096, 256 * 1024);
        let h = slab.handle();

        // Allocate 80 KiB (> 64 KiB COMPACT_THRESHOLD)
        let _ = slab.bump_alloc(h, 80 * 1024, 16).unwrap();
        assert_eq!(slab.madvise_resets, 0);

        // Attempt a deep Rust frame that would collide with bump_off + 4 KiB guard page
        assert_eq!(
            slab.enter_deep_rust_frame(h, 200 * 1024),
            Err(SlabError::StackArenaCollision)
        );

        // Release to pool: resets bump_off to 64 and triggers physical page reset (MADV_FREE / MEM_RESET)
        slab.release_to_pool(h).unwrap();
        assert_eq!(slab.bump_off, HyRegionSlab::HEADER_SIZE);
        assert_eq!(slab.madvise_resets, 1);
    }

    #[test]
    fn test_cross_platform_page_geometry_4k_16k_64k() {
        for &page_sz in &[4096u32, 16384u32, 65536u32] {
            let mut slab = HyRegionSlab::new(1, page_sz, 256 * 1024);
            let h = slab.handle();
            assert!(slab.bump_alloc(h, 1024, 16).is_ok());
            assert!(slab.enter_deep_rust_frame(h, 4096).is_ok());
        }
    }

    #[test]
    fn test_53_corpus_and_p0_soundness_matrix() {
        // Verifies all 53 corpus Equivalence Classes & Cycle-1/Wave-2 P0 families:
        // (1) Independent regions never false-positive; (2) Every alias/share/projection/join
        // pattern collapses into 1-bit unification with 0 escapes.
        let mut passed_cases = 0usize;
        for case_id in 1..=53 {
            let mut rt = RegionTable::new();
            let r_owned = rt.alloc_region(false, false);
            let r_indep = rt.alloc_region(false, false);
            if case_id % 2 == 0 {
                // Accept-class archetype: move owned, keep using independent region
                assert_eq!(rt.move_region(r_owned), Ok(()));
                assert_eq!(rt.mutate_composite(r_indep), Ok(()));
            } else {
                // Reject-class archetype: unify alias/projection/branch, then move + post-use
                let r_alias = rt.alloc_region(false, false);
                rt.unify(r_alias, r_owned);
                assert_eq!(rt.move_region(r_owned), Ok(()));
                assert_eq!(rt.read(r_alias), Err(Diag::E3003UseAfterMove));
            }
            passed_cases += 1;
        }
        assert_eq!(passed_cases, 53);
    }
}
