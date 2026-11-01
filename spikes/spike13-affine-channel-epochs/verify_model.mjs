import assert from 'assert';

class State {
  constructor() {
    this.nextCid = 1;
    this.nextEpoch = 100;
    this.heldTx = new Set();
    this.consumedTx = new Set();
    this.aliases = new Map();
    this.frozenAliases = new Set();
    this.waitForEdges = new Set(); // serialized "from->to"
  }

  allocateChannel() {
    const cid = this.nextCid++;
    const epoch = this.nextEpoch++;
    this.heldTx.add(cid);
    return {
      tx: { type: 'Tx', channel: cid, epoch, isSplit: false },
      rx: { type: 'Rx', channel: cid, epoch }
    };
  }

  aliasTx(aliasName, channelId) {
    this.aliases.set(aliasName, channelId);
  }

  send(tx) {
    if (this.consumedTx.has(tx.channel) || !this.heldTx.has(tx.channel)) {
      throw new Error(`HYB-E3003: Use-after-close on channel ${tx.channel}`);
    }
    return true;
  }

  sendViaAlias(aliasName) {
    if (this.frozenAliases.has(aliasName)) {
      const cid = this.aliases.get(aliasName) || 0;
      throw new Error(`HYB-E3003: Use-after-close on channel ${cid} via frozen alias ${aliasName}`);
    }
    const cid = this.aliases.get(aliasName);
    if (!cid || this.consumedTx.has(cid)) {
      throw new Error(`HYB-E3003: Use-after-close on channel ${cid}`);
    }
    return true;
  }

  close(tx) {
    if (tx.isSplit) {
      throw new Error(`HYB-E3004: Non-owned close on split sub-handle of channel ${tx.channel}`);
    }
    if (this.consumedTx.has(tx.channel) || !this.heldTx.has(tx.channel)) {
      throw new Error(`HYB-E3003: Use-after-close / double-close on channel ${tx.channel}`);
    }
    this.heldTx.delete(tx.channel);
    this.consumedTx.add(tx.channel);

    for (const [name, cid] of this.aliases.entries()) {
      if (cid === tx.channel) {
        this.frozenAliases.add(name);
      }
    }
    return true;
  }

  recordWaitFor(rx, tx, managed = false) {
    const from = rx.channel;
    const to = tx.channel;

    if (managed) {
      this.waitForEdges.add(`${from}->${to}`);
      return true;
    }

    this.waitForEdges.add(`${from}->${to}`);
    const cycle = this.findCycle(from);
    if (cycle) {
      throw new Error(`HYB-E3201: Cyclic channel wait-for dependency detected: ${cycle.join(' -> ')}`);
    }
    return true;
  }

  findCycle(start) {
    const visited = new Set();
    const onStack = new Set();
    const path = [];

    const dfs = (node) => {
      visited.add(node);
      onStack.add(node);
      path.push(node);

      for (const edge of this.waitForEdges) {
        const [fStr, tStr] = edge.split('->');
        const f = Number(fStr);
        const t = Number(tStr);
        if (f === node) {
          if (onStack.has(t)) {
            path.push(t);
            return true;
          }
          if (!visited.has(t) && dfs(t)) {
            return true;
          }
        }
      }

      path.pop();
      onStack.delete(node);
      return false;
    };

    return dfs(start) ? path : null;
  }

  static join(left, right) {
    const merged = new State();
    for (const c of left.consumedTx) merged.consumedTx.add(c);
    for (const c of right.consumedTx) merged.consumedTx.add(c);

    for (const c of left.heldTx) {
      if (right.heldTx.has(c) && !merged.consumedTx.has(c)) {
        merged.heldTx.add(c);
      }
    }

    for (const a of left.frozenAliases) merged.frozenAliases.add(a);
    for (const a of right.frozenAliases) merged.frozenAliases.add(a);

    for (const e of left.waitForEdges) merged.waitForEdges.add(e);
    for (const e of right.waitForEdges) merged.waitForEdges.add(e);

    for (const [k, v] of left.aliases.entries()) merged.aliases.set(k, v);
    for (const [k, v] of right.aliases.entries()) merged.aliases.set(k, v);

    return merged;
  }
}

// -------------------------------------------------------------
// Test Runner
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

console.log('Running endpoint_epoch_model test suite (W-009 / Issue #347)...');

// Part 1: Row 16 Elimination via O5 close(move tx)
runTest('Test 1: Straight-line send after close is rejected (HYB-E3003)', () => {
  const st = new State();
  const { tx } = st.allocateChannel();
  st.close(tx);
  assert.throws(() => st.send(tx), /HYB-E3003/);
});

runTest('Test 2: Double-close on same tx is rejected (HYB-E3003)', () => {
  const st = new State();
  const { tx } = st.allocateChannel();
  st.close(tx);
  assert.throws(() => st.close(tx), /HYB-E3003/);
});

runTest('Test 3: Branch-conditional close invalidates send post-merge (HYB-E3003)', () => {
  const initial = new State();
  const { tx } = initial.allocateChannel();

  // Branch A: leaves open
  const branchA = Object.assign(new State(), {
    heldTx: new Set(initial.heldTx),
    consumedTx: new Set(initial.consumedTx),
  });

  // Branch B: closes
  const branchB = Object.assign(new State(), {
    heldTx: new Set(initial.heldTx),
    consumedTx: new Set(initial.consumedTx),
  });
  branchB.close(tx);

  const merged = State.join(branchA, branchB);
  assert.throws(() => merged.send(tx), /HYB-E3003/);
});

runTest('Test 4: Alias send after move tx is rejected (HYB-E3003 alias freeze)', () => {
  const st = new State();
  const { tx } = st.allocateChannel();
  st.aliasTx('tx2', tx.channel);
  assert.strictEqual(st.sendViaAlias('tx2'), true);

  st.close(tx);
  assert.throws(() => st.sendViaAlias('tx2'), /HYB-E3003/);
});

runTest('Test 5: Split sub-handle cannot call close (HYB-E3004 non-owned close)', () => {
  const st = new State();
  const { tx } = st.allocateChannel();
  const splitTx = { ...tx, isSplit: true };
  assert.throws(() => st.close(splitTx), /HYB-E3004/);
});

runTest('Test 6: Valid producer-consumer pipeline with single-owner close accepted', () => {
  const st = new State();
  const { tx } = st.allocateChannel();
  assert.strictEqual(st.send(tx), true);
  assert.strictEqual(st.send(tx), true);
  assert.strictEqual(st.close(tx), true);
});

// Part 2: Acyclic Wait-For Epochs (HYB-E3201)
runTest('Test 7: Classic 2-task cross-wait deadlock rejected (HYB-E3201)', () => {
  const st = new State();
  const ch1 = st.allocateChannel();
  const ch2 = st.allocateChannel();

  // T1: <-rx1; tx2 <- v
  st.recordWaitFor(ch1.rx, ch2.tx, false);

  // T2: <-rx2; tx1 <- v -> cycle 1 -> 2 -> 1
  assert.throws(() => st.recordWaitFor(ch2.rx, ch1.tx, false), /HYB-E3201/);
});

runTest('Test 8: N-task cyclic dining philosophers ring rejected (HYB-E3201)', () => {
  const st = new State();
  const ch1 = st.allocateChannel();
  const ch2 = st.allocateChannel();
  const ch3 = st.allocateChannel();

  st.recordWaitFor(ch1.rx, ch2.tx, false);
  st.recordWaitFor(ch2.rx, ch3.tx, false);
  assert.throws(() => st.recordWaitFor(ch3.rx, ch1.tx, false), /HYB-E3201/);
});

runTest('Test 9: Idiomatic monotonic multi-stage pipeline accepted', () => {
  const st = new State();
  const ch1 = st.allocateChannel();
  const ch2 = st.allocateChannel();
  const ch3 = st.allocateChannel();

  assert.strictEqual(st.recordWaitFor(ch1.rx, ch2.tx, false), true);
  assert.strictEqual(st.recordWaitFor(ch2.rx, ch3.tx, false), true);
});

runTest('Test 10: Fan-out / Fan-in worker pool (Corpus 15) accepted', () => {
  const st = new State();
  const chWork = st.allocateChannel();
  const chRes = st.allocateChannel();

  assert.strictEqual(st.recordWaitFor(chWork.rx, chRes.tx, false), true);
});

runTest('Test 11: Request-reply pattern with distinct channels accepted', () => {
  const st = new State();
  const chReq = st.allocateChannel();
  const chResp = st.allocateChannel();

  assert.strictEqual(st.recordWaitFor(chReq.rx, chResp.tx, false), true);
});

runTest('Test 12: Intentional cyclic rendezvous permitted via managed hatch', () => {
  const st = new State();
  const ch1 = st.allocateChannel();
  const ch2 = st.allocateChannel();

  st.recordWaitFor(ch1.rx, ch2.tx, false);
  // Allowed under managed hatch
  assert.strictEqual(st.recordWaitFor(ch2.rx, ch1.tx, true), true);
});

console.log(`\nResult: ${passed} passed, ${failed} failed`);
process.exit(failed > 0 ? 1 : 0);
