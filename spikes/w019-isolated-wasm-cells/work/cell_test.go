package w019

import (
	"strings"
	"sync"
	"testing"
	"unsafe"
)

func validDescriptor(name string) ModuleDescriptor {
	return ModuleDescriptor{
		Name:            name,
		ExportsCellStep: true,
	}
}

func echoMutatingHandler(cell *CellInstance, msg WireValue) (WireValue, error) {
	// Mutate cell's own private memory and local decoded slice to test isolation.
	cell.Memory.Bytes[WasmPageBytes-1] ^= 0x5A
	reply := msg.CloneDeep()
	if len(msg.Children) > 0 && len(msg.Children[0].Bytes) > 0 {
		msg.Children[0].Bytes[0] = 0xEE
	}
	return reply, nil
}

func TestZeroSizeWitnesses(t *testing.T) {
	live := MintLive[WireScalarPacket]()
	if sz := unsafe.Sizeof(live); sz != 0 {
		t.Fatalf("expected Live witness size 0, got %d", sz)
	}
	moved, pkt := StaticSend(live, WireScalarPacket{Tag: 1, Value: 42})
	if sz := unsafe.Sizeof(moved); sz != 0 || pkt.Value != 42 {
		t.Fatalf("unexpected witness state: size=%d val=%d", sz, pkt.Value)
	}
}

type snapshotTestEnv struct {
	coord *Coordinator
	cellA *CellInstance
	cellB *CellInstance
}

func TestP1_SnapshotIsolationAcrossCells(t *testing.T) {
	coord := NewCoordinator()
	cellA, err := coord.SpawnCell(validDescriptor("cell-a"), echoMutatingHandler)
	if err != nil {
		t.Fatalf("spawn cell-a: %v", err)
	}
	cellB, err := coord.SpawnCell(validDescriptor("cell-b"), echoMutatingHandler)
	if err != nil {
		t.Fatalf("spawn cell-b: %v", err)
	}

	env := snapshotTestEnv{coord: coord, cellA: cellA, cellB: cellB}
	var wg sync.WaitGroup
	for worker := 0; worker < 8; worker++ {
		wg.Add(1)
		go func(id int) {
			defer wg.Done()
			runConcurrentSnapshotWorker(t, env, id)
		}(worker)
	}
	wg.Wait()
}

func runConcurrentSnapshotWorker(t *testing.T, env snapshotTestEnv, workerID int) {
	for iter := 0; iter < 25; iter++ {
		payload := []byte{byte(workerID), byte(iter), 0xAA, 0xBB}
		msg := WireValue{
			Kind: KindTree,
			Children: []WireValue{
				{Kind: KindBytes, Bytes: payload},
				{Kind: KindScalar, Scalar: int64(iter)},
			},
		}
		replyA, err := env.coord.SendSnapshot(env.cellA, msg)
		if err != nil {
			t.Errorf("SendSnapshot cellA failed: %v", err)
			return
		}
		// Mutate caller's original slice after send; replyA and cellB must not see it.
		payload[0] = 0xFF
		replyB, err := env.coord.SendSnapshot(env.cellB, replyA)
		if err != nil {
			t.Errorf("SendSnapshot cellB failed: %v", err)
			return
		}
		if replyB.Children[0].Bytes[0] != byte(workerID) {
			t.Errorf("cross-cell aliasing detected: got %x, want %x",
				replyB.Children[0].Bytes[0], byte(workerID))
		}
	}
}

func TestP2_FailClosedWireAndModuleAdmission(t *testing.T) {
	verifyRejectedWireKinds(t)
	verifyRejectedWireLimits(t)
	verifyRejectedModuleDescriptors(t)
}

func verifyRejectedWireKinds(t *testing.T) {
	forbidden := []WireKind{KindPtr, KindClosure, KindChan, KindForeignHandle}
	for _, k := range forbidden {
		_, err := AdmitWireValue(WireValue{Kind: k})
		if err == nil || !strings.Contains(err.Error(), ErrCodeWireNotSendable) {
			t.Fatalf("expected %s for kind %v, got %v", ErrCodeWireNotSendable, k, err)
		}
	}
}

func verifyRejectedWireLimits(t *testing.T) {
	deep := WireValue{Kind: KindScalar, Scalar: 7}
	for i := 0; i <= MaxWireDepth; i++ {
		deep = WireValue{Kind: KindTree, Children: []WireValue{deep}}
	}
	if _, err := AdmitWireValue(deep); err == nil || !strings.Contains(err.Error(), ErrCodeWireNotSendable) {
		t.Fatalf("expected %s for depth > %d, got %v", ErrCodeWireNotSendable, MaxWireDepth, err)
	}

	oversized := WireValue{Kind: KindBytes, Bytes: make([]byte, MaxWireFrameBytes)}
	if _, err := AdmitWireValue(oversized); err == nil || !strings.Contains(err.Error(), ErrCodeWireNotSendable) {
		t.Fatalf("expected %s for oversized frame, got %v", ErrCodeWireNotSendable, err)
	}
}

func verifyRejectedModuleDescriptors(t *testing.T) {
	badModules := []ModuleDescriptor{
		{Name: "shared-mem", UsesSharedMemory: true, ExportsCellStep: true},
		{Name: "host-import-mem", ImportsHostMemory: true, ExportsCellStep: true},
		{Name: "ambient-wasi", AmbientWasiImports: []string{"wasi_snapshot_preview1::fd_write"}, ExportsCellStep: true},
	}
	for _, desc := range badModules {
		err := AdmitModule(desc)
		if err == nil || !strings.Contains(err.Error(), ErrCodeModuleViolation) {
			t.Fatalf("expected %s for module %s, got %v", ErrCodeModuleViolation, desc.Name, err)
		}
	}
}

func TestF1_Falsifier_BypassedHostMemoryImportAliases(t *testing.T) {
	coord := NewCoordinator()
	cell1, cell2 := coord.SpawnBypassedSharedMemoryPair(echoMutatingHandler)

	// Write a sentinel into cell1's linear memory; because admission was bypassed
	// to share host memory, cell2 observes the unsynchronized mutation directly.
	cell1.Memory.Bytes[128] = 0x42
	if cell2.Memory.Bytes[128] != 0x42 {
		t.Fatalf("expected bypassed shared memory pair to alias at offset 128")
	}

	// Verify that normal AdmitModule rejects ImportsHostMemory with HYB-E3902.
	err := AdmitModule(ModuleDescriptor{
		Name:              "bypassed-host-mem",
		ImportsHostMemory: true,
		ExportsCellStep:   true,
	})
	if err == nil || !strings.Contains(err.Error(), ErrCodeModuleViolation) {
		t.Fatalf("expected %s on host memory import, got %v", ErrCodeModuleViolation, err)
	}
}

func TestP3_PersistentReactorWithoutReentry(t *testing.T) {
	coord := NewCoordinator()
	var persistentCounter int64
	cell, err := coord.SpawnCell(validDescriptor("reactor"), func(c *CellInstance, msg WireValue) (WireValue, error) {
		persistentCounter += msg.Scalar
		return WireValue{Kind: KindScalar, Scalar: persistentCounter}, nil
	})
	if err != nil {
		t.Fatalf("spawn reactor: %v", err)
	}

	for i := 1; i <= 1000; i++ {
		reply, err := coord.SendSnapshot(cell, WireValue{Kind: KindScalar, Scalar: 1})
		if err != nil {
			t.Fatalf("step %d failed: %v", i, err)
		}
		if reply.Scalar != int64(i) {
			t.Fatalf("step %d counter mismatch: got %d, want %d", i, reply.Scalar, i)
		}
		if cell.ArenaOffset != 0 {
			t.Fatalf("step %d leaked scratch arena offset %d", i, cell.ArenaOffset)
		}
	}
	if cell.StepCount != 1000 {
		t.Fatalf("expected 1000 completed steps on single instance, got %d", cell.StepCount)
	}
}

func TestF2_Falsifier_ReentrantHostCallbackRejected(t *testing.T) {
	coord := NewCoordinator()
	cell, err := coord.SpawnCell(validDescriptor("reentrant-target"), func(c *CellInstance, msg WireValue) (WireValue, error) {
		// Simulate a re-entrant host callback attempting to invoke Step on the same active cell.
		_, reenterErr := c.Step(9)
		return WireValue{Kind: KindScalar, Scalar: 0}, reenterErr
	})
	if err != nil {
		t.Fatalf("spawn cell: %v", err)
	}

	_, err = coord.SendSnapshot(cell, WireValue{Kind: KindScalar, Scalar: 1})
	if err == nil || !strings.Contains(err.Error(), ErrCodeExecutionViolation) {
		t.Fatalf("expected %s on re-entrant callback, got %v", ErrCodeExecutionViolation, err)
	}
}
