package membrane

import (
	"bytes"
	"sync"
	"testing"
)

func TestExclusiveCapabilityAccess(t *testing.T) {
	raw := []byte("hello hybrid zero-copy membrane")
	capA := NewRegionCap[RegionA]()

	m, err := NewBufferMembrane[RegionA](capA, raw)
	if err != nil {
		t.Fatalf("failed to create membrane: %v", err)
	}

	view, err := m.View(capA)
	if err != nil {
		t.Fatalf("expected view access to succeed: %v", err)
	}
	if !bytes.Equal(view, raw) {
		t.Fatalf("view content mismatch: got %s, want %s", string(view), string(raw))
	}

	// In-place mutation through capability
	newData := []byte("HELLO")
	n, err := m.Write(capA, 0, newData)
	if err != nil || n != len(newData) {
		t.Fatalf("write failed: %v", err)
	}

	viewUpdated, _ := m.View(capA)
	if !bytes.HasPrefix(viewUpdated, []byte("HELLO")) {
		t.Fatalf("expected buffer to be updated in place, got: %s", string(viewUpdated))
	}
}

func TestUseAfterMoveRejected(t *testing.T) {
	raw := make([]byte, 1024)
	copy(raw, "secret payload")
	capA := NewRegionCap[RegionA]()

	mA, err := NewBufferMembrane[RegionA](capA, raw)
	if err != nil {
		t.Fatalf("setup failed: %v", err)
	}

	// Linear transfer: RegionA -> RegionB
	mB, capB, err := TransferBuffer[RegionA, RegionB](mA, &capA)
	if err != nil {
		t.Fatalf("transfer failed: %v", err)
	}

	// 1. Destination access succeeds
	vB, err := mB.View(capB)
	if err != nil || !bytes.HasPrefix(vB, []byte("secret payload")) {
		t.Fatalf("destination view failed: %v", err)
	}

	// 2. Source capability is consumed and must be rejected
	_, err = mA.View(capA)
	if err != ErrUseAfterMove {
		t.Fatalf("expected ErrUseAfterMove when reading with consumed cap, got: %v", err)
	}

	_, err = mA.Write(capA, 0, []byte("overwrite"))
	if err != ErrUseAfterMove {
		t.Fatalf("expected ErrUseAfterMove when writing with consumed cap, got: %v", err)
	}

	// 3. Second transfer with consumed capability must be rejected
	_, _, err = TransferBuffer[RegionA, RegionB](mA, &capA)
	if err != ErrUseAfterMove {
		t.Fatalf("expected ErrUseAfterMove on repeated transfer, got: %v", err)
	}
}

func TestStaleRegionEpochRejected(t *testing.T) {
	raw := []byte("epoch test data")
	capValid := NewRegionCap[RegionA]()
	capStale := NewRegionCap[RegionA]() // Different epoch

	m, err := NewBufferMembrane[RegionA](capValid, raw)
	if err != nil {
		t.Fatalf("setup failed: %v", err)
	}

	_, err = m.View(capStale)
	if err != ErrStaleRegion {
		t.Fatalf("expected ErrStaleRegion for mismatched epoch token, got: %v", err)
	}
}

func TestZeroCopyLinearTransferIntegrity(t *testing.T) {
	payloadSize := 1024 * 1024 // 1 MB
	raw := make([]byte, payloadSize)
	raw[0] = 0xAA
	raw[payloadSize-1] = 0xBB

	capA := NewRegionCap[RegionA]()
	mA, err := NewBufferMembrane[RegionA](capA, raw)
	if err != nil {
		t.Fatalf("init failed: %v", err)
	}

	// Hop 1: RegionA -> RegionB
	mB, capB, err := TransferBuffer[RegionA, RegionB](mA, &capA)
	if err != nil {
		t.Fatalf("hop 1 failed: %v", err)
	}
	mB.Write(capB, 10, []byte{0x42})

	// Hop 2: RegionB -> RegionC
	mC, capC, err := TransferBuffer[RegionB, RegionC](mB, &capB)
	if err != nil {
		t.Fatalf("hop 2 failed: %v", err)
	}
	mC.Write(capC, 20, []byte{0x43})

	// Verify terminal buffer integrity
	finalView, err := mC.View(capC)
	if err != nil {
		t.Fatalf("final view failed: %v", err)
	}
	if finalView[0] != 0xAA || finalView[10] != 0x42 || finalView[20] != 0x43 || finalView[payloadSize-1] != 0xBB {
		t.Fatalf("terminal byte corruption detected")
	}
}

func TestConcurrentCrossTaskHandoffUnderRaceDetector(t *testing.T) {
	const iterations = 500
	type TaskMsg struct {
		m   BufferMembrane[RegionA]
		cap RegionCap[RegionA]
	}

	ch := make(chan TaskMsg, 1)

	// Producer
	go func() {
		for i := 0; i < iterations; i++ {
			buf := make([]byte, 1024)
			buf[0] = byte(i & 0xFF)
			c := NewRegionCap[RegionA]()
			m, _ := NewBufferMembrane[RegionA](c, buf)
			ch <- TaskMsg{m: m, cap: c}
		}
		close(ch)
	}()

	// Consumer
	var wg sync.WaitGroup
	wg.Add(1)
	go func() {
		defer wg.Done()
		received := 0
		for msg := range ch {
			v, err := msg.m.View(msg.cap)
			if err != nil {
				t.Errorf("consumer view failed: %v", err)
			}
			if len(v) != 1024 {
				t.Errorf("corrupt length: %d", len(v))
			}
			received++
		}
		if received != iterations {
			t.Errorf("received count mismatch: %d != %d", received, iterations)
		}
	}()

	wg.Wait()
}

func TestEnterRegionScopeLifecycle(t *testing.T) {
	var leakedCap RegionCap[RegionA]
	var m BufferMembrane[RegionA]

	val, err := EnterRegion[RegionA, int](func(cap RegionCap[RegionA]) (int, error) {
		leakedCap = cap
		var err error
		m, err = NewBufferMembrane[RegionA](cap, []byte("scoped data"))
		if err != nil {
			return 0, err
		}
		view, err := m.View(cap)
		if err != nil {
			return 0, err
		}
		return len(view), nil
	})

	if err != nil || val != 11 {
		t.Fatalf("scoped execution failed: val=%d, err=%v", val, err)
	}

	// After EnterRegion returns, leakedCap must be consumed
	_, err = m.View(leakedCap)
	if err != ErrUseAfterMove {
		t.Fatalf("expected ErrUseAfterMove for capability used outside EnterRegion scope, got: %v", err)
	}
}
