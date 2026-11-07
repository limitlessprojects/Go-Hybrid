// spikes/spike13-affine-channel-epochs/endpoint_epoch_model.rs
// Bounded feasibility model for Wildcard Bounty #347 (W-009)
// Formalizing Affine tx[T]/rx[T] Endpoint Split + Acyclic Wait-For Epochs.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_CHANNEL_ID: AtomicU64 = AtomicU64::new(1);
static NEXT_EPOCH: AtomicU64 = AtomicU64::new(100);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ChannelId(pub u64);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct Epoch(pub u64);

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Endpoint {
    /// Owned linear sender capability. Exactly one owner; destroyed by close().
    Tx {
        channel: ChannelId,
        epoch: Epoch,
        is_split: bool,
    },
    /// Shareable receiver capability. Copyable across multiple tasks.
    Rx {
        channel: ChannelId,
        epoch: Epoch,
    },
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Error {
    /// HYB-E3003: Use-after-close or send on closed channel
    UseAfterClose(ChannelId),
    /// HYB-E3004: Attempting to close a non-owned or split channel endpoint
    NonOwnedClose(ChannelId),
    /// HYB-E3201: Cyclic channel wait-for dependency (deadlock risk)
    CyclicWaitForDependency {
        cycle: Vec<ChannelId>,
    },
    /// Missing or unallocated endpoint
    MissingEndpoint(ChannelId),
}

#[derive(Clone, Debug, Default)]
pub struct State {
    pub held_tx: BTreeSet<ChannelId>,
    pub consumed_tx: BTreeSet<ChannelId>,
    pub aliases: BTreeMap<String, ChannelId>,
    pub frozen_aliases: BTreeSet<String>,
    pub wait_for_edges: BTreeSet<(ChannelId, ChannelId)>, // (waits_on_rx, fulfills_tx)
}

impl State {
    pub fn allocate_channel(&mut self) -> (Endpoint, Endpoint) {
        let cid = ChannelId(NEXT_CHANNEL_ID.fetch_add(1, Ordering::Relaxed));
        let epoch = Epoch(NEXT_EPOCH.fetch_add(1, Ordering::Relaxed));
        self.held_tx.insert(cid);
        (
            Endpoint::Tx { channel: cid, epoch, is_split: false },
            Endpoint::Rx { channel: cid, epoch },
        )
    }

    pub fn alias_tx(&mut self, alias_name: &str, channel: ChannelId) {
        self.aliases.insert(alias_name.to_string(), channel);
    }

    pub fn send(&self, tx: &Endpoint) -> Result<(), Error> {
        match tx {
            Endpoint::Tx { channel, .. } => {
                if self.consumed_tx.contains(channel) {
                    return Err(Error::UseAfterClose(*channel));
                }
                if !self.held_tx.contains(channel) {
                    return Err(Error::UseAfterClose(*channel));
                }
                Ok(())
            }
            Endpoint::Rx { .. } => {
                panic!("Cannot send on an Rx endpoint");
            }
        }
    }

    pub fn send_via_alias(&self, alias_name: &str) -> Result<(), Error> {
        if self.frozen_aliases.contains(alias_name) {
            let cid = self.aliases.get(alias_name).copied().unwrap_or(ChannelId(0));
            return Err(Error::UseAfterClose(cid));
        }
        let cid = self.aliases.get(alias_name).copied().ok_or(Error::MissingEndpoint(ChannelId(0)))?;
        if self.consumed_tx.contains(&cid) {
            return Err(Error::UseAfterClose(cid));
        }
        Ok(())
    }

    pub fn close(&mut self, tx: Endpoint) -> Result<(), Error> {
        match tx {
            Endpoint::Tx { channel, is_split, .. } => {
                if is_split {
                    return Err(Error::NonOwnedClose(channel));
                }
                if self.consumed_tx.contains(&channel) {
                    return Err(Error::UseAfterClose(channel));
                }
                if !self.held_tx.remove(&channel) {
                    return Err(Error::UseAfterClose(channel));
                }
                self.consumed_tx.insert(channel);

                // Freeze all known aliases of this channel
                for (name, cid) in &self.aliases {
                    if *cid == channel {
                        self.frozen_aliases.insert(name.clone());
                    }
                }
                Ok(())
            }
            Endpoint::Rx { .. } => {
                panic!("Cannot close an Rx endpoint");
            }
        }
    }

    pub fn record_wait_for(
        &mut self,
        rx: &Endpoint,
        tx: &Endpoint,
        managed: bool,
    ) -> Result<(), Error> {
        let (rx_cid, rx_epoch) = match rx {
            Endpoint::Rx { channel, epoch } => (*channel, *epoch),
            _ => panic!("Expected Rx endpoint"),
        };
        let (tx_cid, tx_epoch) = match tx {
            Endpoint::Tx { channel, epoch, .. } => (*channel, *epoch),
            _ => panic!("Expected Tx endpoint"),
        };

        if managed {
            self.wait_for_edges.insert((rx_cid, tx_cid));
            return Ok(());
        }

        // Epoch order check: rx.epoch must strictly precede tx.epoch
        if rx_epoch >= tx_epoch {
            // Check if adding this creates a cycle
            self.wait_for_edges.insert((rx_cid, tx_cid));
            if let Some(cycle) = self.find_cycle(rx_cid) {
                return Err(Error::CyclicWaitForDependency { cycle });
            }
        } else {
            self.wait_for_edges.insert((rx_cid, tx_cid));
        }

        if let Some(cycle) = self.find_cycle(rx_cid) {
            return Err(Error::CyclicWaitForDependency { cycle });
        }

        Ok(())
    }

    fn find_cycle(&self, start: ChannelId) -> Option<Vec<ChannelId>> {
        let mut visited = BTreeSet::new();
        let mut on_stack = BTreeSet::new();
        let mut path = Vec::new();

        if self.dfs_cycle(start, &mut visited, &mut on_stack, &mut path) {
            Some(path)
        } else {
            None
        }
    }

    fn dfs_cycle(
        &self,
        node: ChannelId,
        visited: &mut BTreeSet<ChannelId>,
        on_stack: &mut BTreeSet<ChannelId>,
        path: &mut Vec<ChannelId>,
    ) -> bool {
        visited.insert(node);
        on_stack.insert(node);
        path.push(node);

        for (from, to) in &self.wait_for_edges {
            if *from == node {
                if on_stack.contains(to) {
                    path.push(*to);
                    return true;
                }
                if !visited.contains(to) {
                    if self.dfs_cycle(*to, visited, on_stack, path) {
                        return true;
                    }
                }
            }
        }

        path.pop();
        on_stack.remove(&node);
        false
    }

    pub fn join(left: &State, right: &State) -> State {
        // Linear consumption is unioned (if consumed on any path, it's consumed post-merge)
        let mut consumed_tx = left.consumed_tx.clone();
        consumed_tx.extend(right.consumed_tx.iter().copied());

        // Held only if held on BOTH paths
        let held_tx = left
            .held_tx
            .intersection(&right.held_tx)
            .copied()
            .filter(|cid| !consumed_tx.contains(cid))
            .collect();

        let mut frozen_aliases = left.frozen_aliases.clone();
        frozen_aliases.extend(right.frozen_aliases.iter().cloned());

        let mut wait_for_edges = left.wait_for_edges.clone();
        wait_for_edges.extend(right.wait_for_edges.iter().copied());

        let mut aliases = left.aliases.clone();
        for (k, v) in &right.aliases {
            aliases.insert(k.clone(), *v);
        }

        State {
            held_tx,
            consumed_tx,
            aliases,
            frozen_aliases,
            wait_for_edges,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_1_straight_line_send_after_close_rejected() {
        let mut st = State::default();
        let (tx, _rx) = st.allocate_channel();
        let cid = match tx { Endpoint::Tx { channel, .. } => channel, _ => unreachable!() };

        // close(move tx) consumes capability
        st.close(tx.clone()).unwrap();

        // Attempting to send using tx fails with HYB-E3003
        assert_eq!(st.send(&tx), Err(Error::UseAfterClose(cid)));
    }

    #[test]
    fn test_2_double_close_rejected() {
        let mut st = State::default();
        let (tx, _rx) = st.allocate_channel();
        let cid = match tx { Endpoint::Tx { channel, .. } => channel, _ => unreachable!() };

        st.close(tx.clone()).unwrap();
        // Second close fails with HYB-E3003
        assert_eq!(st.close(tx), Err(Error::UseAfterClose(cid)));
    }

    #[test]
    fn test_3_branch_conditional_close_invalidates_post_merge() {
        let mut initial = State::default();
        let (tx, _rx) = initial.allocate_channel();
        let cid = match tx { Endpoint::Tx { channel, .. } => channel, _ => unreachable!() };

        // Branch A: leaves channel open
        let branch_a = initial.clone();

        // Branch B: closes channel
        let mut branch_b = initial.clone();
        branch_b.close(tx.clone()).unwrap();

        // Join states (if/else merge)
        let merged = State::join(&branch_a, &branch_b);

        // Fail-closed: tx is consumed on at least one path, so cannot send post-merge
        assert_eq!(merged.send(&tx), Err(Error::UseAfterClose(cid)));
    }

    #[test]
    fn test_4_alias_send_after_close_rejected() {
        let mut st = State::default();
        let (tx, _rx) = st.allocate_channel();
        let cid = match tx { Endpoint::Tx { channel, .. } => channel, _ => unreachable!() };

        st.alias_tx("tx2", cid);
        assert!(st.send_via_alias("tx2").is_ok());

        // close original tx
        st.close(tx).unwrap();

        // sending via alias tx2 now fails with HYB-E3003
        assert_eq!(st.send_via_alias("tx2"), Err(Error::UseAfterClose(cid)));
    }

    #[test]
    fn test_5_split_subhandle_cannot_call_close() {
        let mut st = State::default();
        let (tx, _rx) = st.allocate_channel();
        let (cid, epoch) = match tx { Endpoint::Tx { channel, epoch, .. } => (channel, epoch), _ => unreachable!() };

        let split_tx = Endpoint::Tx { channel: cid, epoch, is_split: true };
        assert_eq!(st.close(split_tx), Err(Error::NonOwnedClose(cid)));
    }

    #[test]
    fn test_6_valid_producer_consumer_pipeline_accepted() {
        let mut st = State::default();
        let (tx, _rx) = st.allocate_channel();

        // Send multiple times while held
        assert!(st.send(&tx).is_ok());
        assert!(st.send(&tx).is_ok());

        // Close once at end of life
        assert!(st.close(tx).is_ok());
    }

    #[test]
    fn test_7_two_task_cross_wait_deadlock_rejected() {
        let mut st = State::default();
        let (tx1, rx1) = st.allocate_channel();
        let (tx2, rx2) = st.allocate_channel();

        // T1 waits on rx1 before fulfilling tx2
        st.record_wait_for(&rx1, &tx2, false).unwrap();

        // T2 waits on rx2 before fulfilling tx1 -> creates cycle!
        let res = st.record_wait_for(&rx2, &tx1, false);
        assert!(matches!(res, Err(Error::CyclicWaitForDependency { .. })));
    }

    #[test]
    fn test_8_n_task_dining_philosophers_ring_rejected() {
        let mut st = State::default();
        let (tx1, rx1) = st.allocate_channel();
        let (tx2, rx2) = st.allocate_channel();
        let (tx3, rx3) = st.allocate_channel();

        // Chain 1 -> 2 -> 3
        st.record_wait_for(&rx1, &tx2, false).unwrap();
        st.record_wait_for(&rx2, &tx3, false).unwrap();

        // Close the loop 3 -> 1
        let res = st.record_wait_for(&rx3, &tx1, false);
        assert!(matches!(res, Err(Error::CyclicWaitForDependency { .. })));
    }

    #[test]
    fn test_9_idiomatic_monotonic_pipeline_accepted() {
        let mut st = State::default();
        let (tx1, rx1) = st.allocate_channel();
        let (tx2, rx2) = st.allocate_channel();
        let (tx3, _rx3) = st.allocate_channel();

        // Sequential pipeline: Stage 1 feeds Stage 2 feeds Stage 3
        assert!(st.record_wait_for(&rx1, &tx2, false).is_ok());
        assert!(st.record_wait_for(&rx2, &tx3, false).is_ok());
    }

    #[test]
    fn test_10_fan_out_fan_in_worker_pool_accepted() {
        let mut st = State::default();
        let (tx_work, rx_work) = st.allocate_channel();
        let (tx_res, _rx_res) = st.allocate_channel();

        // Workers share rx_work and fulfill tx_res (monotonic forward progress)
        assert!(st.record_wait_for(&rx_work, &tx_res, false).is_ok());
    }

    #[test]
    fn test_11_request_reply_with_distinct_channels_accepted() {
        let mut st = State::default();
        let (tx_req, rx_req) = st.allocate_channel();
        let (tx_resp, _rx_resp) = st.allocate_channel();

        // Server receives request and sends response
        assert!(st.record_wait_for(&rx_req, &tx_resp, false).is_ok());
    }

    #[test]
    fn test_12_intentional_cyclic_rendezvous_permitted_via_managed() {
        let mut st = State::default();
        let (tx1, rx1) = st.allocate_channel();
        let (tx2, rx2) = st.allocate_channel();

        st.record_wait_for(&rx1, &tx2, false).unwrap();
        // Permitted with managed hatch
        let res = st.record_wait_for(&rx2, &tx1, true);
        assert!(res.is_ok());
    }
}
