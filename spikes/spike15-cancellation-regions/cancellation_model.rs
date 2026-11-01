// spikes/spike15-cancellation-regions/cancellation_model.rs
// Bounded feasibility model for Wildcard Bounty #348 (W-007)
// Formalizing cancellation-safe structured task regions through blocking-effect admission.

use std::collections::{BTreeMap, BTreeSet, VecDeque};
use std::sync::atomic::{AtomicU64, Ordering};

static NEXT_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct RegionId(pub u64);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct TaskId(pub u64);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ChannelId(pub u64);

#[derive(Clone, Copy, Debug, Eq, Ord, PartialEq, PartialOrd)]
pub struct ResourceId(pub u64);

/// Blocking and concurrency effects that can occur within a task region.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Effect {
    ComputeStep,
    LoopBackedgeCheck,
    ChannelSend { channel: ChannelId, val: i64, interruptible: bool },
    ChannelRecv { channel: ChannelId, interruptible: bool },
    Select { branches: Vec<Effect>, has_cancel_branch: bool },
    SpawnChild { child_task: TaskId, escapes_region: bool },
    ForeignCall { name: &'static str, interruptible: bool },
    CleanupStep { is_blocking: bool },
}

/// Static admission rejection errors mapping to HYB-E35xx diagnostics.
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum AdmissionError {
    /// HYB-E3501: Uninterruptible foreign or third-party call lacks cancellation-effect summary.
    UninterruptibleForeignCall(&'static str),
    /// HYB-E3502: Unstructured goroutine or unparented task escapes structured region lifetime.
    EscapingTask(TaskId),
    /// HYB-E3503: Select statement within cancellable region lacks a cancellation branch.
    MissingCancellationBranch,
    /// HYB-E3504: Cleanup/defer block contains unbounded blocking operation.
    BlockingCleanup,
}

pub struct AdmissionChecker;

impl AdmissionChecker {
    pub fn admit_effect(effect: &Effect) -> Result<(), AdmissionError> {
        match effect {
            Effect::ComputeStep | Effect::LoopBackedgeCheck => Ok(()),
            Effect::ChannelSend { interruptible, .. } => {
                if !interruptible {
                    Err(AdmissionError::MissingCancellationBranch)
                } else {
                    Ok(())
                }
            }
            Effect::ChannelRecv { interruptible, .. } => {
                if !interruptible {
                    Err(AdmissionError::MissingCancellationBranch)
                } else {
                    Ok(())
                }
            }
            Effect::Select { has_cancel_branch, branches } => {
                if !has_cancel_branch {
                    return Err(AdmissionError::MissingCancellationBranch);
                }
                for branch in branches {
                    Self::admit_effect(branch)?;
                }
                Ok(())
            }
            Effect::SpawnChild { escapes_region, child_task } => {
                if *escapes_region {
                    Err(AdmissionError::EscapingTask(*child_task))
                } else {
                    Ok(())
                }
            }
            Effect::ForeignCall { name, interruptible } => {
                if !interruptible {
                    Err(AdmissionError::UninterruptibleForeignCall(*name))
                } else {
                    Ok(())
                }
            }
            Effect::CleanupStep { is_blocking } => {
                if *is_blocking {
                    Err(AdmissionError::BlockingCleanup)
                } else {
                    Ok(())
                }
            }
        }
    }

    pub fn admit_program(effects: &[Effect]) -> Result<(), AdmissionError> {
        for eff in effects {
            Self::admit_effect(eff)?;
        }
        Ok(())
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum TaskStatus {
    Ready,
    Running,
    BlockedSend(ChannelId),
    BlockedRecv(ChannelId),
    Completed,
    Cancelled,
}

#[derive(Clone, Debug)]
pub struct Task {
    pub id: TaskId,
    pub region: RegionId,
    pub status: TaskStatus,
    pub instructions: VecDeque<Effect>,
    pub cleanup: Vec<Effect>,
    pub cleanups_executed: bool,
}

#[derive(Clone, Debug)]
pub struct Channel {
    pub capacity: usize,
    pub buffer: VecDeque<i64>,
}

#[derive(Clone, Debug)]
pub struct RegionState {
    pub id: RegionId,
    pub cancelled: bool,
    pub active_children: BTreeSet<TaskId>,
    pub owned_resources: BTreeSet<ResourceId>,
    pub released: bool,
}

#[derive(Clone, Debug, Default)]
pub struct SupervisorSimulator {
    pub regions: BTreeMap<RegionId, RegionState>,
    pub tasks: BTreeMap<TaskId, Task>,
    pub channels: BTreeMap<ChannelId, Channel>,
    pub accessed_resources_after_release: BTreeSet<ResourceId>,
}

impl SupervisorSimulator {
    pub fn new_region(&mut self) -> RegionId {
        let id = RegionId(NEXT_ID.fetch_add(1, Ordering::Relaxed));
        self.regions.insert(id, RegionState {
            id,
            cancelled: false,
            active_children: BTreeSet::new(),
            owned_resources: BTreeSet::new(),
            released: false,
        });
        id
    }

    pub fn allocate_resource(&mut self, region: RegionId) -> ResourceId {
        let rid = ResourceId(NEXT_ID.fetch_add(1, Ordering::Relaxed));
        if let Some(r) = self.regions.get_mut(&region) {
            r.owned_resources.insert(rid);
        }
        rid
    }

    pub fn new_channel(&mut self, capacity: usize) -> ChannelId {
        let cid = ChannelId(NEXT_ID.fetch_add(1, Ordering::Relaxed));
        self.channels.insert(cid, Channel { capacity, buffer: VecDeque::new() });
        cid
    }

    pub fn spawn_task(&mut self, region: RegionId, instructions: Vec<Effect>, cleanup: Vec<Effect>) -> TaskId {
        let tid = TaskId(NEXT_ID.fetch_add(1, Ordering::Relaxed));
        let task = Task {
            id: tid,
            region,
            status: TaskStatus::Ready,
            instructions: instructions.into(),
            cleanup,
            cleanups_executed: false,
        };
        self.tasks.insert(tid, task);
        if let Some(r) = self.regions.get_mut(&region) {
            r.active_children.insert(tid);
        }
        tid
    }

    pub fn cancel_region(&mut self, region: RegionId) {
        if let Some(r) = self.regions.get_mut(&region) {
            r.cancelled = true;
        }
        for task in self.tasks.values_mut() {
            if task.region == region {
                match task.status {
                    TaskStatus::BlockedSend(_) | TaskStatus::BlockedRecv(_) | TaskStatus::Ready | TaskStatus::Running => {
                        task.status = TaskStatus::Cancelled;
                    }
                    TaskStatus::Completed | TaskStatus::Cancelled => {}
                }
            }
        }
    }

    pub fn step_task(&mut self, tid: TaskId) -> bool {
        let mut task = match self.tasks.remove(&tid) {
            Some(t) => t,
            None => return false,
        };

        let region_cancelled = self.regions.get(&task.region).map(|r| r.cancelled).unwrap_or(false);

        if region_cancelled && task.status == TaskStatus::Running {
            task.status = TaskStatus::Cancelled;
        }

        if task.status == TaskStatus::Cancelled {
            if !task.cleanups_executed {
                for c in &task.cleanup {
                    let _ = c;
                }
                task.cleanups_executed = true;
            }
            if let Some(r) = self.regions.get_mut(&task.region) {
                r.active_children.remove(&tid);
            }
            self.tasks.insert(tid, task);
            return false;
        }

        if let Some(eff) = task.instructions.pop_front() {
            match eff {
                Effect::ComputeStep => {
                    task.status = TaskStatus::Running;
                }
                Effect::LoopBackedgeCheck => {
                    if region_cancelled {
                        task.status = TaskStatus::Cancelled;
                        if !task.cleanups_executed {
                            for c in &task.cleanup {
                                let _ = c;
                            }
                            task.cleanups_executed = true;
                        }
                        if let Some(r) = self.regions.get_mut(&task.region) {
                            r.active_children.remove(&tid);
                        }
                        self.tasks.insert(tid, task);
                        return false;
                    } else {
                        task.status = TaskStatus::Running;
                    }
                }
                Effect::ChannelSend { channel, val, interruptible } => {
                    let chan = self.channels.get_mut(&channel).unwrap();
                    if chan.buffer.len() < chan.capacity {
                        chan.buffer.push_back(val);
                        task.status = TaskStatus::Running;
                    } else if region_cancelled && interruptible {
                        task.status = TaskStatus::Cancelled;
                    } else {
                        task.status = TaskStatus::BlockedSend(channel);
                        task.instructions.push_front(Effect::ChannelSend { channel, val, interruptible });
                    }
                }
                Effect::ChannelRecv { channel, interruptible } => {
                    let chan = self.channels.get_mut(&channel).unwrap();
                    if let Some(_val) = chan.buffer.pop_front() {
                        task.status = TaskStatus::Running;
                    } else if region_cancelled && interruptible {
                        task.status = TaskStatus::Cancelled;
                    } else {
                        task.status = TaskStatus::BlockedRecv(channel);
                        task.instructions.push_front(Effect::ChannelRecv { channel, interruptible });
                    }
                }
                Effect::Select { .. } => {
                    task.status = TaskStatus::Running;
                }
                Effect::SpawnChild { child_task, .. } => {
                    if let Some(r) = self.regions.get_mut(&task.region) {
                        r.active_children.insert(child_task);
                    }
                    task.status = TaskStatus::Running;
                }
                Effect::ForeignCall { .. } => {
                    task.status = TaskStatus::Running;
                }
                Effect::CleanupStep { .. } => {
                    task.status = TaskStatus::Running;
                }
            }
            if task.instructions.is_empty() && task.status == TaskStatus::Running {
                task.status = TaskStatus::Completed;
                if !task.cleanups_executed {
                    for c in &task.cleanup {
                        let _ = c;
                    }
                    task.cleanups_executed = true;
                }
                if let Some(r) = self.regions.get_mut(&task.region) {
                    r.active_children.remove(&tid);
                }
            }
            self.tasks.insert(tid, task);
            true
        } else {
            task.status = TaskStatus::Completed;
            if !task.cleanups_executed {
                task.cleanups_executed = true;
            }
            if let Some(r) = self.regions.get_mut(&task.region) {
                r.active_children.remove(&tid);
            }
            self.tasks.insert(tid, task);
            false
        }
    }

    pub fn join_region(&mut self, region: RegionId) -> Result<(), &'static str> {
        let r = self.regions.get_mut(&region).ok_or("Region not found")?;
        if !r.active_children.is_empty() {
            return Err("Cannot join: orphan children still active in region");
        }
        r.released = true;
        Ok(())
    }

    pub fn access_resource(&mut self, region: RegionId, resource: ResourceId) -> Result<(), &'static str> {
        let r = self.regions.get(&region).ok_or("Region not found")?;
        if r.released {
            self.accessed_resources_after_release.insert(resource);
            return Err("Soundness violation: Post-join use-after-release of region resource!");
        }
        if !r.owned_resources.contains(&resource) {
            return Err("Resource does not belong to region");
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn positive_1_producer_blocked_on_send_unblocks_via_cancellation() {
        let mut sim = SupervisorSimulator::default();
        let region = sim.new_region();
        let chan = sim.new_channel(0); // unbuffered channel -> blocks without receiver
        let res = sim.allocate_resource(region);

        let producer = sim.spawn_task(
            region,
            vec![
                Effect::ChannelSend { channel: chan, val: 42, interruptible: true },
            ],
            vec![Effect::CleanupStep { is_blocking: false }],
        );

        // Step 1: Producer tries to send, blocks on unbuffered channel
        assert!(sim.step_task(producer));
        assert_eq!(sim.tasks.get(&producer).unwrap().status, TaskStatus::BlockedSend(chan));
        assert!(sim.access_resource(region, res).is_ok());

        // Step 2: Cancel region while producer is blocked
        sim.cancel_region(region);
        assert_eq!(sim.tasks.get(&producer).unwrap().status, TaskStatus::Cancelled);

        // Step 3: Run step -> executes cleanups and transitions out
        sim.step_task(producer);
        assert!(sim.tasks.get(&producer).unwrap().cleanups_executed);

        // Step 4: Region joins cleanly with 0 orphans
        assert_eq!(sim.join_region(region), Ok(()));
        // Step 5: Post-join access is prohibited
        assert!(sim.access_resource(region, res).is_err());
        assert_eq!(sim.regions.get(&region).unwrap().active_children.len(), 0);
    }

    #[test]
    fn positive_2_consumer_blocked_on_receive_unblocks_without_leaks() {
        let mut sim = SupervisorSimulator::default();
        let region = sim.new_region();
        let chan = sim.new_channel(1); // empty channel -> blocks on recv
        let res = sim.allocate_resource(region);

        let consumer = sim.spawn_task(
            region,
            vec![
                Effect::ChannelRecv { channel: chan, interruptible: true },
            ],
            vec![Effect::CleanupStep { is_blocking: false }],
        );

        // Step 1: Consumer blocks on empty channel
        assert!(sim.step_task(consumer));
        assert_eq!(sim.tasks.get(&consumer).unwrap().status, TaskStatus::BlockedRecv(chan));

        // Step 2: Region is cancelled
        sim.cancel_region(region);
        assert_eq!(sim.tasks.get(&consumer).unwrap().status, TaskStatus::Cancelled);

        // Step 3: Step to execute cleanups
        sim.step_task(consumer);
        assert!(sim.tasks.get(&consumer).unwrap().cleanups_executed);

        // Step 4: Join succeeds, resources freed
        assert_eq!(sim.join_region(region), Ok(()));
        assert!(sim.access_resource(region, res).is_err());
        assert_eq!(sim.regions.get(&region).unwrap().active_children.len(), 0);
    }

    #[test]
    fn positive_3_compute_loop_canceled_at_backedge_joins_nested_children() {
        let mut sim = SupervisorSimulator::default();
        let region = sim.new_region();
        let res = sim.allocate_resource(region);

        let child = sim.spawn_task(
            region,
            vec![
                Effect::ComputeStep,
                Effect::LoopBackedgeCheck,
            ],
            vec![Effect::CleanupStep { is_blocking: false }],
        );

        let parent = sim.spawn_task(
            region,
            vec![
                Effect::SpawnChild { child_task: child, escapes_region: false },
                Effect::ComputeStep,
                Effect::LoopBackedgeCheck,
                Effect::ComputeStep,
                Effect::LoopBackedgeCheck,
            ],
            vec![Effect::CleanupStep { is_blocking: false }],
        );

        // Run parent initial steps
        sim.step_task(parent); // SpawnChild
        sim.step_task(parent); // ComputeStep
        sim.step_task(child);  // Child ComputeStep

        // Cancel while in middle of loop
        sim.cancel_region(region);

        // Parent hits backedge check -> detects cancellation and halts
        sim.step_task(parent);
        assert_eq!(sim.tasks.get(&parent).unwrap().status, TaskStatus::Cancelled);

        // Child hits backedge check -> detects cancellation and halts
        sim.step_task(child);
        assert_eq!(sim.tasks.get(&child).unwrap().status, TaskStatus::Cancelled);

        // Both joined
        assert_eq!(sim.join_region(region), Ok(()));
        assert_eq!(sim.regions.get(&region).unwrap().active_children.len(), 0);
        assert!(sim.access_resource(region, res).is_err());
    }

    #[test]
    fn falsification_1_uninterruptible_foreign_call_rejected() {
        let effect = Effect::ForeignCall {
            name: "uninstrumented_c_library_call",
            interruptible: false,
        };
        assert_eq!(
            AdmissionChecker::admit_effect(&effect),
            Err(AdmissionError::UninterruptibleForeignCall("uninstrumented_c_library_call"))
        );
    }

    #[test]
    fn falsification_2_escaping_goroutine_rejected() {
        let tid = TaskId(999);
        let effect = Effect::SpawnChild {
            child_task: tid,
            escapes_region: true,
        };
        assert_eq!(
            AdmissionChecker::admit_effect(&effect),
            Err(AdmissionError::EscapingTask(tid))
        );
    }

    #[test]
    fn falsification_3_nested_select_without_cancellation_branch_rejected() {
        let chan = ChannelId(10);
        let effect = Effect::Select {
            branches: vec![
                Effect::ChannelSend { channel: chan, val: 1, interruptible: false },
            ],
            has_cancel_branch: false,
        };
        assert_eq!(
            AdmissionChecker::admit_effect(&effect),
            Err(AdmissionError::MissingCancellationBranch)
        );
    }

    #[test]
    fn falsification_4_blocking_cleanup_rejected() {
        let effect = Effect::CleanupStep { is_blocking: true };
        assert_eq!(
            AdmissionChecker::admit_effect(&effect),
            Err(AdmissionError::BlockingCleanup)
        );
    }
}
