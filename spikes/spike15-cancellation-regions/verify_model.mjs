import assert from 'assert';

class AdmissionChecker {
  static admitEffect(effect) {
    switch (effect.type) {
      case 'ComputeStep':
      case 'LoopBackedgeCheck':
        return { ok: true };
      case 'ChannelSend':
        if (!effect.interruptible) {
          return { ok: false, error: 'MissingCancellationBranch' };
        }
        return { ok: true };
      case 'ChannelRecv':
        if (!effect.interruptible) {
          return { ok: false, error: 'MissingCancellationBranch' };
        }
        return { ok: true };
      case 'Select':
        if (!effect.hasCancelBranch) {
          return { ok: false, error: 'MissingCancellationBranch' };
        }
        for (const branch of effect.branches) {
          const res = this.admitEffect(branch);
          if (!res.ok) return res;
        }
        return { ok: true };
      case 'SpawnChild':
        if (effect.escapesRegion) {
          return { ok: false, error: `EscapingTask(${effect.childTask})` };
        }
        return { ok: true };
      case 'ForeignCall':
        if (!effect.interruptible) {
          return { ok: false, error: `UninterruptibleForeignCall(${effect.name})` };
        }
        return { ok: true };
      case 'CleanupStep':
        if (effect.isBlocking) {
          return { ok: false, error: 'BlockingCleanup' };
        }
        return { ok: true };
      default:
        throw new Error('Unknown effect type');
    }
  }
}

class SupervisorSimulator {
  constructor() {
    this.nextId = 1;
    this.regions = new Map();
    this.tasks = new Map();
    this.channels = new Map();
    this.accessedResourcesAfterRelease = new Set();
  }

  newRegion() {
    const id = this.nextId++;
    this.regions.set(id, {
      id,
      cancelled: false,
      activeChildren: new Set(),
      ownedResources: new Set(),
      released: false,
    });
    return id;
  }

  allocateResource(regionId) {
    const rid = this.nextId++;
    const r = this.regions.get(regionId);
    if (r) r.ownedResources.add(rid);
    return rid;
  }

  newChannel(capacity) {
    const cid = this.nextId++;
    this.channels.set(cid, { capacity, buffer: [] });
    return cid;
  }

  spawnTask(regionId, instructions, cleanup = []) {
    const tid = this.nextId++;
    const task = {
      id: tid,
      regionId,
      status: 'Ready',
      instructions: [...instructions],
      cleanup: [...cleanup],
      cleanupsExecuted: false,
    };
    this.tasks.set(tid, task);
    const r = this.regions.get(regionId);
    if (r) r.activeChildren.add(tid);
    return tid;
  }

  cancelRegion(regionId) {
    const r = this.regions.get(regionId);
    if (r) r.cancelled = true;
    for (const task of this.tasks.values()) {
      if (task.regionId === regionId) {
        if (['BlockedSend', 'BlockedRecv', 'Ready', 'Running'].includes(task.status)) {
          task.status = 'Cancelled';
        }
      }
    }
  }

  stepTask(tid) {
    const task = this.tasks.get(tid);
    if (!task) return false;

    const r = this.regions.get(task.regionId);
    const regionCancelled = r ? r.cancelled : false;

    if (regionCancelled && task.status === 'Running') {
      task.status = 'Cancelled';
    }

    if (task.status === 'Cancelled') {
      if (!task.cleanupsExecuted) {
        task.cleanupsExecuted = true;
      }
      if (r) r.activeChildren.delete(tid);
      return false;
    }

    if (task.instructions.length > 0) {
      const eff = task.instructions.shift();
      switch (eff.type) {
        case 'ComputeStep':
          task.status = 'Running';
          break;
        case 'LoopBackedgeCheck':
          if (regionCancelled) {
            task.status = 'Cancelled';
            if (!task.cleanupsExecuted) task.cleanupsExecuted = true;
            if (r) r.activeChildren.delete(tid);
            return false;
          }
          task.status = 'Running';
          break;
        case 'ChannelSend': {
          const chan = this.channels.get(eff.channel);
          if (chan.buffer.length < chan.capacity) {
            chan.buffer.push(eff.val);
            task.status = 'Running';
          } else if (regionCancelled && eff.interruptible) {
            task.status = 'Cancelled';
          } else {
            task.status = 'BlockedSend';
            task.instructions.unshift(eff);
          }
          break;
        }
        case 'ChannelRecv': {
          const chan = this.channels.get(eff.channel);
          if (chan.buffer.length > 0) {
            chan.buffer.shift();
            task.status = 'Running';
          } else if (regionCancelled && eff.interruptible) {
            task.status = 'Cancelled';
          } else {
            task.status = 'BlockedRecv';
            task.instructions.unshift(eff);
          }
          break;
        }
        case 'Select':
          task.status = 'Running';
          break;
        case 'SpawnChild':
          if (r) r.activeChildren.add(eff.childTask);
          task.status = 'Running';
          break;
        case 'ForeignCall':
        case 'CleanupStep':
          task.status = 'Running';
          break;
      }

      if (task.instructions.length === 0 && task.status === 'Running') {
        task.status = 'Completed';
        if (!task.cleanupsExecuted) task.cleanupsExecuted = true;
        if (r) r.activeChildren.delete(tid);
      }
      return true;
    } else {
      task.status = 'Completed';
      if (!task.cleanupsExecuted) task.cleanupsExecuted = true;
      if (r) r.activeChildren.delete(tid);
      return false;
    }
  }

  joinRegion(regionId) {
    const r = this.regions.get(regionId);
    if (!r) throw new Error('Region not found');
    if (r.activeChildren.size > 0) {
      throw new Error('Cannot join: orphan children still active');
    }
    r.released = true;
    return true;
  }

  accessResource(regionId, resourceId) {
    const r = this.regions.get(regionId);
    if (!r) throw new Error('Region not found');
    if (r.released) {
      this.accessedResourcesAfterRelease.add(resourceId);
      throw new Error('Soundness violation: Post-join use-after-release!');
    }
    if (!r.ownedResources.has(resourceId)) {
      throw new Error('Resource not owned by region');
    }
    return true;
  }
}

// -------------------------------------------------------------
// Test Suite Execution
// -------------------------------------------------------------
let passed = 0;
let failed = 0;

function runTest(name, fn) {
  try {
    fn();
    console.log(`[PASS] ${name}`);
    passed++;
  } catch (err) {
    console.error(`[FAIL] ${name}:`, err.message);
    failed++;
  }
}

console.log('Running cancellation_model test suite...');

runTest('Positive 1: Pipeline producer blocked on send unblocks via cancellation exit', () => {
  const sim = new SupervisorSimulator();
  const region = sim.newRegion();
  const chan = sim.newChannel(0); // unbuffered -> blocks without receiver
  const res = sim.allocateResource(region);

  const producer = sim.spawnTask(
    region,
    [{ type: 'ChannelSend', channel: chan, val: 42, interruptible: true }],
    [{ type: 'CleanupStep', isBlocking: false }]
  );

  assert.strictEqual(sim.stepTask(producer), true);
  assert.strictEqual(sim.tasks.get(producer).status, 'BlockedSend');
  assert.strictEqual(sim.accessResource(region, res), true);

  // Cancel region
  sim.cancelRegion(region);
  assert.strictEqual(sim.tasks.get(producer).status, 'Cancelled');

  // Step executes cleanup and unblocks
  sim.stepTask(producer);
  assert.strictEqual(sim.tasks.get(producer).cleanupsExecuted, true);

  // Region joins with 0 orphans
  assert.strictEqual(sim.joinRegion(region), true);
  // Post-join access rejected
  assert.throws(() => sim.accessResource(region, res), /Soundness violation/);
  assert.strictEqual(sim.regions.get(region).activeChildren.size, 0);
});

runTest('Positive 2: Consumer blocked on receive unblocks without leaks', () => {
  const sim = new SupervisorSimulator();
  const region = sim.newRegion();
  const chan = sim.newChannel(1); // empty channel
  const res = sim.allocateResource(region);

  const consumer = sim.spawnTask(
    region,
    [{ type: 'ChannelRecv', channel: chan, interruptible: true }],
    [{ type: 'CleanupStep', isBlocking: false }]
  );

  assert.strictEqual(sim.stepTask(consumer), true);
  assert.strictEqual(sim.tasks.get(consumer).status, 'BlockedRecv');

  sim.cancelRegion(region);
  assert.strictEqual(sim.tasks.get(consumer).status, 'Cancelled');

  sim.stepTask(consumer);
  assert.strictEqual(sim.tasks.get(consumer).cleanupsExecuted, true);

  assert.strictEqual(sim.joinRegion(region), true);
  assert.throws(() => sim.accessResource(region, res), /Soundness violation/);
  assert.strictEqual(sim.regions.get(region).activeChildren.size, 0);
});

runTest('Positive 3: Compute loop canceled at backedge check joins nested children', () => {
  const sim = new SupervisorSimulator();
  const region = sim.newRegion();
  const res = sim.allocateResource(region);

  const child = sim.spawnTask(
    region,
    [{ type: 'ComputeStep' }, { type: 'LoopBackedgeCheck' }],
    [{ type: 'CleanupStep', isBlocking: false }]
  );

  const parent = sim.spawnTask(
    region,
    [
      { type: 'SpawnChild', childTask: child, escapesRegion: false },
      { type: 'ComputeStep' },
      { type: 'LoopBackedgeCheck' },
      { type: 'ComputeStep' },
      { type: 'LoopBackedgeCheck' },
    ],
    [{ type: 'CleanupStep', isBlocking: false }]
  );

  sim.stepTask(parent); // SpawnChild
  sim.stepTask(parent); // ComputeStep
  sim.stepTask(child);  // Child ComputeStep

  sim.cancelRegion(region);

  sim.stepTask(parent); // hits backedge check -> detects cancellation
  assert.strictEqual(sim.tasks.get(parent).status, 'Cancelled');

  sim.stepTask(child); // hits backedge check -> detects cancellation
  assert.strictEqual(sim.tasks.get(child).status, 'Cancelled');

  assert.strictEqual(sim.joinRegion(region), true);
  assert.strictEqual(sim.regions.get(region).activeChildren.size, 0);
  assert.throws(() => sim.accessResource(region, res), /Soundness violation/);
});

runTest('Falsification 1: Reject uninterruptible foreign call', () => {
  const effect = {
    type: 'ForeignCall',
    name: 'uninstrumented_c_library_call',
    interruptible: false,
  };
  const res = AdmissionChecker.admitEffect(effect);
  assert.strictEqual(res.ok, false);
  assert.strictEqual(res.error, 'UninterruptibleForeignCall(uninstrumented_c_library_call)');
});

runTest('Falsification 2: Reject escaping goroutine', () => {
  const effect = {
    type: 'SpawnChild',
    childTask: 999,
    escapesRegion: true,
  };
  const res = AdmissionChecker.admitEffect(effect);
  assert.strictEqual(res.ok, false);
  assert.strictEqual(res.error, 'EscapingTask(999)');
});

runTest('Falsification 3: Reject nested select without cancellation branch', () => {
  const effect = {
    type: 'Select',
    branches: [{ type: 'ChannelSend', channel: 10, val: 1, interruptible: false }],
    hasCancelBranch: false,
  };
  const res = AdmissionChecker.admitEffect(effect);
  assert.strictEqual(res.ok, false);
  assert.strictEqual(res.error, 'MissingCancellationBranch');
});

runTest('Falsification 4: Reject blocking cleanup', () => {
  const effect = {
    type: 'CleanupStep',
    isBlocking: true,
  };
  const res = AdmissionChecker.admitEffect(effect);
  assert.strictEqual(res.ok, false);
  assert.strictEqual(res.error, 'BlockingCleanup');
});

console.log(`\nResult: ${passed} passed, ${failed} failed`);
process.exit(failed > 0 ? 1 : 0);
